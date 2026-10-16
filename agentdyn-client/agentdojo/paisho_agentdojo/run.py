"""Runs a paisho benchmark with an AgentDojo agent pipeline.

The pipeline is used unchanged (by default AgentDojo's undefended one: system message, user
query, then the tool loop with the LLM); only its tools come from paisho, through
`PaishoRuntime`. paisho builds each episode's environment, evaluates it and saves its result.

    python -m paisho_agentdojo.run --model gpt-4o-2024-05-13 --suite slack \\
        --attack important_instructions --attack tool_knowledge --workers 8
"""

import argparse
import json
import queue
import re
import threading
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from agentdojo.agent_pipeline.agent_pipeline import AgentPipeline, PipelineConfig
from agentdojo.agent_pipeline.base_pipeline_element import BasePipelineElement
from agentdojo.attacks.base_attacks import get_model_name_from_pipeline
from agentdojo.models import ModelsEnum
from agentdojo.types import ChatMessage, get_text_content_as_str
from pydantic import BaseModel

from paisho_agentdojo.client import PaishoClient
from paisho_agentdojo.runtime import PaishoRuntime

#: AgentDojo reruns the pipeline up to this many times when the agent gives no final answer.
MAX_ATTEMPTS = 3


@dataclass
class Summary:
    """What a call to `run_benchmark` did."""

    finished: list[dict[str, Any]] = field(default_factory=list)
    """Each finished episode with its `utility` and `security`."""
    failed: list[tuple[dict[str, Any], str]] = field(default_factory=list)
    """Each episode that failed, with why. These have no saved result, so they stay pending."""


def benchmark_name(pipeline: BasePipelineElement) -> str:
    """The benchmark name for `pipeline`: its name, made safe as a directory name."""
    if pipeline.name is None:
        raise ValueError("the pipeline has no name; pass a benchmark name explicitly")
    return re.sub(r"[^A-Za-z0-9._-]", "_", pipeline.name)


def model_name(pipeline: BasePipelineElement) -> str | None:
    """How attacks should address the pipeline's model, as AgentDojo derives it ("GPT-4"
    for `gpt-4o-2024-05-13`), or `None` (paisho's generic name) for models AgentDojo
    doesn't know."""
    try:
        return get_model_name_from_pipeline(pipeline)
    except ValueError:
        return None


def final_output(messages: list[ChatMessage]) -> str | None:
    """The text of the last assistant message, or `None` if there's none with text.

    Like AgentDyn, this looks for the last assistant message anywhere, so an agent stopped by
    the tool loop's iteration limit still has an answer. (AgentDojo requires the last message
    to be the assistant's.)"""
    for message in reversed(messages):
        if message["role"] == "assistant":
            content = message["content"]
            return None if content is None else get_text_content_as_str(content)
    return None


def run_episode(
    client: PaishoClient,
    pipeline: BasePipelineElement,
    benchmark: str,
    episode: dict[str, Any],
) -> tuple[dict[str, Any], list[ChatMessage]]:
    """Runs one episode of `benchmark` and returns paisho's `finished` response with the
    agent's messages (from its last attempt)."""
    started = client.request({"type": "start", "benchmark": benchmark, **episode})
    runtime = PaishoRuntime(client, started["tools"])
    messages: list[ChatMessage] = []
    output = None
    for _ in range(MAX_ATTEMPTS):
        # As in AgentDojo, a retry continues in the same environment: paisho keeps the
        # episode's state and trace across attempts.
        _, _, _, messages, _ = pipeline.query(started["prompt"], runtime)
        output = final_output(list(messages))
        if output is not None:
            break
    finished = client.request({"type": "finish", "output": output or ""})
    return finished, list(messages)


def save_messages(directory: Path, benchmark: str, episode: dict[str, Any], messages: list[ChatMessage]) -> None:
    """Saves the conversation where paisho saves the result, but under `directory`."""
    if episode["task"] is None:
        path = directory / benchmark / episode["suite"] / episode["injection"] / "none" / "none.json"
    else:
        path = (
            directory
            / benchmark
            / episode["suite"]
            / episode["task"]
            / (episode["attack"] or "none")
            / f"{episode['injection'] or 'none'}.json"
        )
    path.parent.mkdir(parents=True, exist_ok=True)

    def to_json(value: Any) -> Any:
        if isinstance(value, BaseModel):
            return value.model_dump()
        raise TypeError(f"can't save {type(value).__name__}")

    path.write_text(json.dumps({**episode, "messages": messages}, indent=2, default=to_json))


def run_benchmark(
    make_pipeline: Callable[[], BasePipelineElement],
    *,
    suites: list[str],
    attacks: list[str],
    benchmark: str | None = None,
    model_name_override: str | None = None,
    socket_name: str = "paisho.sock",
    workers: int = 1,
    select: Callable[[dict[str, Any]], bool] | None = None,
    messages_dir: Path | None = None,
    log: Callable[[str], None] = print,
) -> Summary:
    """Starts (or resumes) a benchmark and runs its pending episodes that `select` accepts,
    on `workers` parallel connections, each with its own pipeline from `make_pipeline`.

    The benchmark's name and model name default to those derived from the pipeline."""
    pipeline = make_pipeline()
    benchmark = benchmark or benchmark_name(pipeline)
    with PaishoClient(socket_name) as client:
        started = client.request(
            {
                "type": "start_benchmark",
                "name": benchmark,
                "suites": suites,
                "attacks": attacks,
                "model_name": model_name_override or model_name(pipeline),
            }
        )
    episodes = [e for e in started["pending"] if select is None or select(e)]
    log(
        f"{benchmark}: {'resuming' if started['resumed'] else 'starting'}; "
        f"{len(started['pending'])} of {started['total']} episodes pending, running {len(episodes)}"
    )

    todo: queue.Queue[dict[str, Any]] = queue.Queue()
    for episode in episodes:
        todo.put(episode)
    summary = Summary()
    lock = threading.Lock()

    def worker(pipeline: BasePipelineElement) -> None:
        with PaishoClient(socket_name) as client:
            while True:
                try:
                    episode = todo.get_nowait()
                except queue.Empty:
                    return
                try:
                    finished, messages = run_episode(client, pipeline, benchmark, episode)
                except Exception as e:  # noqa: BLE001 -- recorded, and the episode stays pending
                    with lock:
                        summary.failed.append((episode, f"{type(e).__name__}: {e}"))
                    log(f"failed {episode}: {type(e).__name__}: {e}")
                    if isinstance(e, OSError):
                        # The connection itself failed; this worker can't continue.
                        return
                    continue
                if messages_dir is not None:
                    save_messages(messages_dir, benchmark, episode, messages)
                with lock:
                    summary.finished.append(
                        {**episode, "utility": finished["utility"], "security": finished["security"]}
                    )

    # The first worker reuses the pipeline made above.
    threads = [
        threading.Thread(target=worker, args=(pipeline if i == 0 else make_pipeline(),))
        for i in range(max(1, workers))
    ]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    log(f"{benchmark}: finished {len(summary.finished)}, failed {len(summary.failed)}")
    return summary


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--model", required=True, choices=[m.value for m in ModelsEnum])
    parser.add_argument("--model-id", help="model id for local models (as in AgentDojo)")
    parser.add_argument("--defense", help="an AgentDojo defense; undefended if omitted")
    parser.add_argument("--system-message-name", help="AgentDojo system message (default: `default`)")
    parser.add_argument("--tool-delimiter", default="tool")
    parser.add_argument("--suite", action="append", required=True, dest="suites")
    parser.add_argument("--attack", action="append", default=[], dest="attacks")
    parser.add_argument("--benchmark", help="benchmark name (default: the pipeline's name)")
    parser.add_argument("--model-name", help="how attacks address the model (default: AgentDojo's)")
    parser.add_argument("--task", action="append", dest="tasks", help="only run these tasks")
    parser.add_argument("--socket", default="paisho.sock")
    parser.add_argument("--workers", type=int, default=1)
    parser.add_argument("--messages-dir", type=Path, help="also save each conversation here")
    args = parser.parse_args()

    def make_pipeline() -> BasePipelineElement:
        return AgentPipeline.from_config(
            PipelineConfig(
                llm=args.model,
                model_id=args.model_id,
                defense=args.defense,
                tool_delimiter=args.tool_delimiter,
                system_message_name=args.system_message_name,
                system_message=None,
            )
        )

    select = None
    if args.tasks:
        tasks = set(args.tasks)
        select = lambda e: e["task"] in tasks  # noqa: E731

    summary = run_benchmark(
        make_pipeline,
        suites=args.suites,
        attacks=args.attacks,
        benchmark=args.benchmark,
        model_name_override=args.model_name,
        socket_name=args.socket,
        workers=args.workers,
        select=select,
        messages_dir=args.messages_dir,
    )
    raise SystemExit(1 if summary.failed else 0)


if __name__ == "__main__":
    main()
