# paisho ❀

paisho aims to be a generic, extensible, and relatively high-performance (read: parallel) benchmarking environment for agentic systems security. It is focused on prompt injection attacks and is heavily influenced by [AgentDojo](https://arxiv.org/abs/2406.13352) and [AgentDyn](https://arxiv.org/pdf/2602.03117).

The [paisho/](./paisho) directory contains the benchmarking environment.

The [salmon/](./salmon) directory contains a simple agent harness interfacing with paisho, along with a Racket-based policy specification language and associated security monitor implementation. It serves as both an example use-case of paisho and an artifact of independent interest.
