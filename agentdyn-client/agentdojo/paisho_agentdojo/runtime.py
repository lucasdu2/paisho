"""An AgentDojo `FunctionsRuntime` whose tools are an episode's tools on a paisho server.

AgentDojo's pipelines use a runtime in exactly two ways: LLM adapters read
`runtime.functions` (each function's `name`, `description` and
`parameters.model_json_schema()`) to declare tools to the model, and `ToolsExecutor` calls
`runtime.run_function(env, name, args)`. This runtime builds `functions` from the tool
schemas paisho returns when an episode starts, and runs every call on the server.
"""

from typing import Any

from agentdojo.functions_runtime import Env, Function, FunctionReturnType, FunctionsRuntime
from pydantic import BaseModel

from paisho_agentdojo.client import PaishoClient


def _parameters_model(name: str, schema: dict[str, Any]) -> type[BaseModel]:
    """A Pydantic model whose JSON schema is `schema` as paisho gave it. The adapters only
    ever call `model_json_schema()` on a function's parameters, so the model needs no
    fields of its own."""

    class Parameters(BaseModel):
        @classmethod
        def model_json_schema(cls, *args: Any, **kwargs: Any) -> dict[str, Any]:
            return schema

    Parameters.__name__ = f"{name}_parameters"
    return Parameters


def _not_called(**kwargs: Any) -> None:
    raise AssertionError("paisho runs tools; `PaishoRuntime.run_function` never calls this")


class PaishoRuntime(FunctionsRuntime):
    """Tools of the episode in progress on `client`, from the `tools` of its `started`
    response."""

    def __init__(self, client: PaishoClient, tools: list[dict[str, Any]]) -> None:
        super().__init__([])
        self.client = client
        for tool in tools:
            self.functions[tool["name"]] = Function(
                name=tool["name"],
                description=tool["description"],
                parameters=_parameters_model(tool["name"], tool["input_schema"]),
                dependencies={},
                run=_not_called,
                full_docstring=tool["description"],
                return_type=None,
            )

    def run_function(
        self,
        env: Env | None,
        function: str,
        kwargs: dict[str, Any],
        raise_on_error: bool = False,
    ) -> tuple[FunctionReturnType, str | None]:
        """Runs `function` on the server. Returns its text and no error, or an empty result
        and the error text, as AgentDojo's runtime does. `env` is unused: the environment
        lives on the server."""
        response = self.client.request_raw(
            {"type": "call_tool", "name": function, "arguments": kwargs}
        )
        if response["type"] == "error":
            error = response["message"]
        else:
            result = response["result"]
            text = "\n".join(block["text"] for block in result["content"])
            if not result["isError"]:
                return text, None
            error = text
        if raise_on_error:
            raise RuntimeError(error)
        return "", error
