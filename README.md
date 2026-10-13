# paisho ❀

paisho aims to be a generic, extensible, and relatively high-performance (read: parallel) benchmarking environment for agentic systems security. It is focused on prompt injection attacks intended to expose security and/or privacy vulnerabilities in the agentic setting, e.g., indirect prompt injection and control flow hijacking. It is heavily influenced by AgentDojo and AgentDyn.

The paisho/ directory contains the benchmarking environment.

The salmon/ directory contains a simple agent harness interfacing with paisho, along with a Racket-based policy specification language and associated security monitor implementation. It serves as both an example use-case of paisho and an artifact of independent interest.
