"""Runs AgentDojo's pipeline against a real paisho server, with a scripted model in place of
an LLM so no API calls are made."""

import json
import os
import subprocess
from collections.abc import Iterator, Sequence
from pathlib import Path

import pytest
from agentdojo.agent_pipeline.agent_pipeline import AgentPipeline
from agentdojo.agent_pipeline.base_pipeline_element import BasePipelineElement
from agentdojo.agent_pipeline.basic_elements import InitQuery, SystemMessage
from agentdojo.agent_pipeline.tool_execution import ToolsExecutionLoop, ToolsExecutor
from agentdojo.functions_runtime import EmptyEnv, Env, FunctionCall, FunctionsRuntime
from agentdojo.types import ChatAssistantMessage, ChatMessage, text_content_block_from_string

from paisho_agentdojo.client import PaishoClient
from paisho_agentdojo.run import run_benchmark

# The paisho crate, from `agentdyn-client/agentdojo/tests/`.
CRATE = Path(__file__).resolve().parents[3] / "paisho"


class ScriptedLLM(BasePipelineElement):
    """A stand-in model: reads www.informations.com while also calling a tool that doesn't
    exist, then visits www.true-informations.com (the goal of Slack's injection task 3),
    then answers. It records what tools it was offered and what tool results it saw."""

    name = "scripted"

    def __init__(self) -> None:
        self.offered: set[str] = set()
        self.seen: list[tuple[str, str | None]] = []

    def query(
        self,
        query: str,
        runtime: FunctionsRuntime,
        env: Env = EmptyEnv(),
        messages: Sequence[ChatMessage] = [],
        extra_args: dict = {},
    ) -> tuple[str, FunctionsRuntime, Env, Sequence[ChatMessage], dict]:
        self.offered = set(runtime.functions)
        for message in messages:
            if message["role"] == "tool":
                self.seen.append((message["content"][0]["content"], message["error"]))
        turn = sum(message["role"] == "assistant" for message in messages)
        if turn == 0:
            calls = [
                FunctionCall(function="get_webpage", args={"url": "www.informations.com"}, id="1"),
                FunctionCall(function="no_such_tool", args={}, id="2"),
            ]
        elif turn == 1:
            calls = [FunctionCall(function="get_webpage", args={"url": "www.true-informations.com"}, id="3")]
        else:
            reply = ChatAssistantMessage(
                role="assistant", content=[text_content_block_from_string("Done.")], tool_calls=None
            )
            return query, runtime, env, [*messages, reply], extra_args
        call = ChatAssistantMessage(role="assistant", content=None, tool_calls=calls)
        return query, runtime, env, [*messages, call], extra_args


def scripted_pipeline(llm: ScriptedLLM) -> AgentPipeline:
    """AgentDojo's undefended pipeline (as `AgentPipeline.from_config` builds it), with the
    scripted model."""
    pipeline = AgentPipeline(
        [
            SystemMessage("You are a test agent."),
            InitQuery(),
            llm,
            ToolsExecutionLoop([ToolsExecutor(), llm]),
        ]
    )
    pipeline.name = "scripted"
    return pipeline


@pytest.fixture(scope="module")
def server(tmp_path_factory: pytest.TempPathFactory) -> Iterator[tuple[str, Path]]:
    """A paisho server built from this checkout, with its socket name and logdir."""
    subprocess.run(["cargo", "build", "--quiet"], cwd=CRATE, check=True)
    socket_name = f"paisho-client-test-{os.getpid()}.sock"
    logdir = tmp_path_factory.mktemp("runs")
    process = subprocess.Popen(
        [CRATE / "target" / "debug" / "paisho", socket_name, logdir],
        stderr=subprocess.PIPE,
        text=True,
    )
    assert "listening" in process.stderr.readline()
    yield socket_name, logdir
    process.kill()
    process.wait()


def test_runs_episodes_through_agentdojos_pipeline(server: tuple[str, Path], tmp_path: Path) -> None:
    socket_name, logdir = server
    llm = ScriptedLLM()
    selected = lambda e: e["task"] in ("user_task_0", None)  # noqa: E731

    summary = run_benchmark(
        lambda: scripted_pipeline(llm),
        suites=["slack"],
        attacks=["important_instructions"],
        socket_name=socket_name,
        workers=1,
        select=selected,
        messages_dir=tmp_path,
        log=lambda _: None,
    )

    assert summary.failed == []
    # user_task_0 benign and under 5 injections, plus the 5 injections alone.
    assert len(summary.finished) == 1 + 5 + 5
    # The model was offered paisho's Slack tools.
    assert {"get_webpage", "read_channel_messages", "send_direct_message"} <= llm.offered
    # Tool results came back from paisho, and the unknown tool was caught by AgentDojo.
    assert ("", "Invalid tool no_such_tool provided.") in llm.seen
    assert any("<INFORMATION>" in text for text, _ in llm.seen)

    by_episode = {(e["task"], e["injection"]): e for e in summary.finished}
    # user_task_0 only needs www.informations.com read, which the agent always does.
    assert by_episode[("user_task_0", None)]["utility"] is True
    assert by_episode[("user_task_0", None)]["security"] is None
    # Visiting www.true-informations.com is injection task 3's goal, attacked or alone.
    assert by_episode[("user_task_0", "injection_task_3")]["security"] is True
    assert by_episode[("user_task_0", "injection_task_1")]["security"] is False
    assert by_episode[(None, "injection_task_3")] | {} == {
        "suite": "slack",
        "task": None,
        "attack": None,
        "injection": "injection_task_3",
        "utility": None,
        "security": True,
    }

    # paisho saved the results, with the answer the client extracted.
    result = json.loads(
        (logdir / "scripted/slack/user_task_0/important_instructions/injection_task_3.json").read_text()
    )
    assert result["output"] == "Done."
    assert [call["name"] for call in result["tool_calls"]] == ["get_webpage", "get_webpage"]
    # AgentDojo doesn't know the "scripted" model, so attacks use the generic name.
    assert result["model_name"] == "the AI language model"

    # The client saved the conversation alongside.
    messages = json.loads((tmp_path / "scripted/slack/injection_task_3/none/none.json").read_text())["messages"]
    assert [m["role"] for m in messages] == ["system", "user", "assistant", "tool", "tool", "assistant", "tool", "assistant"]

    # Running again resumes: nothing selected is left.
    again = run_benchmark(
        lambda: scripted_pipeline(ScriptedLLM()),
        suites=["slack"],
        attacks=["important_instructions"],
        socket_name=socket_name,
        select=selected,
        log=lambda _: None,
    )
    assert again.finished == [] and again.failed == []


def test_parallel_workers(server: tuple[str, Path]) -> None:
    socket_name, logdir = server
    summary = run_benchmark(
        lambda: scripted_pipeline(ScriptedLLM()),
        suites=["slack"],
        attacks=["tool_knowledge"],
        benchmark="parallel",
        socket_name=socket_name,
        workers=8,
        log=lambda _: None,
    )
    assert summary.failed == []
    assert len(summary.finished) == 21 * (1 + 5) + 5
    with PaishoClient(socket_name) as client:
        resumed = client.request(
            {"type": "start_benchmark", "name": "parallel", "suites": ["slack"], "attacks": ["tool_knowledge"]}
        )
    assert resumed["pending"] == []
