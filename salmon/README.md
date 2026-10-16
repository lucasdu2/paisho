# salmon 𓆝 𓆟 𓆞 𓆝 𓆟

A *s*tateful *a*nd *l*ogical language for implementing agentic security *mon*itors.

## Agent harness

salmon includes a simple agent harness that runs benchmarks against a paisho server. It
follows AgentDojo's (and AgentDyn's) undefended harness: AgentDyn's system prompt, the
user's prompt, then up to 15 rounds in which every tool call of the model's reply is run and
its result sent back.

- `paisho.rkt`: a client for paisho's socket protocol.
- `llm.rkt`: a provider-neutral conversation and the interface model adapters implement.
- `openai.rkt`: an adapter for OpenAI's Chat Completions API with native tool calling. Its
  base URL is configurable, so it also works with OpenAI-compatible services (OpenRouter,
  vLLM, ...).
- `agent.rkt`: the agent loop.
- `main.rkt`: runs a benchmark, with episodes in parallel.

```sh
# In ../paisho: start the server.
cargo run --release -- paisho.sock runs

# Here (the API key is read from OPENAI_API_KEY, or the variable --api-key-env names):
racket main.rkt --model gpt-4o-2024-08-06 --suite slack \
    --attack important_instructions --attack tool_knowledge --workers 8
```

Use `--base-url https://openrouter.ai/api/v1 --api-key-env OPENROUTER_API_KEY` for
OpenRouter, and `--no-temperature` for models that reject a temperature. See `--help` for
the other options.

Tests use a fake OpenAI server on localhost and a real paisho server built from this
checkout, so they make no API calls:

```sh
raco test tests/
```
