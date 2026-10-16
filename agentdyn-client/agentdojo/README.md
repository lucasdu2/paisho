# paisho-agentdojo

Runs paisho benchmarks with the agent pipelines of [AgentDyn](https://github.com/SaFo-Lab/AgentDyn),
an extension of [AgentDojo](https://github.com/ethz-spylab/agentdojo). The pipeline is used
unchanged (by default the undefended one); only its tools come from paisho, through
`PaishoRuntime`. paisho builds each episode's environment,
evaluates the episode and saves its result.

[!NOTE]
This client is entirely Claude-generated, with no real oversight. It is throwaway code that is not designed to be maintained. Also: this client directly pulls in AgentDyn's agent harness, which supports older models that maybe deprecated/

paisho runs as a separate server process; this package talks to it over its socket.

```sh
# In paisho/: start the server (socket name and results directory are optional).
cargo run --release -- paisho.sock runs

# Here, in a fresh virtual environment:
pip install -e .
python -m paisho_agentdojo.run --model gpt-4o-2024-05-13 --suite slack \
    --attack important_instructions --attack tool_knowledge --workers 8
```

Installing pulls in AgentDyn's harness pinned to an upstream commit (see `pyproject.toml`).
AgentDyn's package is also named `agentdojo`, so it replaces any other `agentdojo` in the
environment; use a separate environment for this client. `--model` accepts that commit's
models.

The benchmark is named after the pipeline (here `gpt-4o-2024-05-13`) unless `--benchmark`
is given. Running the same command again resumes it. See `--help` for the other options,
including `--defense` for AgentDojo's defenses and `--messages-dir` to keep conversations.

Tests run a real paisho server (built with `cargo build`) against AgentDojo's pipeline with
a scripted model, so they make no API calls:

```sh
pip install -e '.[test]'
python -m pytest tests
```
