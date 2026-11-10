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
- `responses.rkt`: an adapter for OpenAI's Responses API (`--api responses`), for models
  that only take tools with reasoning through it, such as gpt-6-luna. Requests are
  stateless, and the model's reasoning is sent back on later turns in encrypted form.
- `agent.rkt`: the agent loop.
- `main.rkt`: runs a benchmark, with episodes in parallel.

```sh
# In ../paisho: start the server.
cargo run --release -- paisho.sock runs

# Here (the API key is read from OPENAI_API_KEY, or the variable --api-key-env names):
racket main.rkt --model gpt-4o-2024-08-06 --suite slack \
    --attack important_instructions --attack tool_knowledge --workers 8
```

Each episode run appends a line to `usage/<benchmark>.jsonl` with the episode, its utility
and security (or its error, since a failed episode's calls are paid for too) and the tokens
its model calls used: `calls`, `input_tokens`, `cached_input_tokens` (the part of the input
served from the provider's prompt cache) and `output_tokens`. A resumed run appends to the
same file, and the run's totals are printed at the end. Use `--usage-log FILE` to write it
elsewhere, or `--no-usage-log` to skip it.

Each finished episode also appends a line to `usage/<benchmark>.conversations.jsonl` with
the episode, the system prompt and the whole conversation: the user's prompt, the model's
replies (text and tool calls) and the tool results. paisho's saved results hold only the
final answer and the tool calls, so this is where text from earlier turns is kept. An episode
rerun because the model gave no final answer logs its last attempt; a failed episode logs
nothing. Use `--conversation-log FILE` to write it elsewhere, or `--no-conversation-log` to
skip it.

Use `--base-url https://openrouter.ai/api/v1 --api-key-env OPENROUTER_API_KEY` for
OpenRouter, and `--no-temperature` for models that reject a temperature. With
`--api responses`, no temperature is sent unless given. `--reasoning-effort` sets the model's
reasoning effort with either API; `--reasoning-effort none` turns reasoning off, which
gpt-6-luna needs in order to take tools through Chat Completions (`--api chat`, the
default). See `--help` for
the other options.

Tests use a fake OpenAI server on localhost and a real paisho server built from this
checkout, so they make no API calls:

```sh
raco test tests/
```
