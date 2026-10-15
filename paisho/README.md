# paisho ❀
paisho is a partial Rust re-implementation of the AgentDojo agentic security benchmark, with some changes and extensions.

## Motivation and Overall Design
AgentDojo is currently (for better or worse) the de-facto benchmark for empirical evaluation of research prototypes for agentic security, particularly defenses against indirect prompt injection. However, the design of AgentDojo is (in my view) unsatisfactory: its implementation needlessly couples the actual benchmarking functionality with a built-in framework for implementing custom agent pipelines and defense mechanisms; and the benchmarking code contains a good deal of object-oriented cruft and complexity that is due, in part, to the aforementioned coupling.

In practice, this means that novel defenses must either (a) use AgentDojo's built-in framework out-of-the-box, constraining their design decisions and forcing them to use Python (see CaMeL, Progent, etc.), (b) shoehorn their defense implementation into AgentDojo's particular design using an ad-hoc interfacing layer (see TypeGuard and LLMbda), or (c) simply reimplement AgentDojo's benchmarks in their own particular choice of programming language (see Tacit by Odersky et al.). None of these options are ideal with regards to reusability and flexibility, and all are rooted in the violation of the principle of separation-of-concerns.

The goal of paisho is to port the *benchmark portions* of AgentDojo wholesale, eliminating some of the object-oriented cruft and complexity of the original Python implementation, and fully uncoupling the evaluation machinery from the actual implementation of a defense.

paisho *only* implements AgentDojo's built-in set of mock environments and evaluation suites. It then provides a uniform JSON interface (based on the standard JSON-RPC format that protocols like MCP have settled on) to interact with these environments and evaluation suites. This creates a more flexible (and more traditional, in a sense) client-server structure, where defenses are clients that call paisho as a server. As long as the defenses use the generic JSON interface, they are free to use any programming language and design in their implementation. A smaller secondary goal is to provide a better default evaluation metric---one that evaluates utility and security together at a per-run level, instead of in isolation and as a mean over all runs (as AgentDojo is currently configured to do).

### Other differences from AgentDojo
paisho aims to differ from AgentDojo in a few other ways:
- Remove the DIFFICULTY annotation on tasks. A brief survey of work that builds on AgentDojo finds that this annotation is rarely used; indeed, the AgentDojo paper itself doesn't use it. Difficulty can be better approximated by other metrics, which can be dynamically tracked: these include number of turns required to complete a task (length) and the number of dynamic replanning steps required for a task. (Both these metrics are taken from the AgentDyn extension.)
- Only include the latest version of each default suite. Versioning is no longer done through hard-coded directories; instead, it can be done through Git commit hashes or actual version releases.
- Include all AgentDyn suites.
- Parallel execution of tasks (instead of just parallelism at the suite level, which seems to be broken in AgentDojo anyways).
- Easy resumption of benchmarks if the run is stopped unexpectedly (like reaching a token limit).
