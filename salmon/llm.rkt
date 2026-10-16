#lang racket/base

;; A provider-neutral conversation and the interface every model adapter implements. Adapters
;; convert to and from their provider's format in one place; the agent loop only sees these.

(provide (struct-out tool)
         (struct-out tool-call)
         (struct-out user-message)
         (struct-out assistant-message)
         (struct-out tool-message)
         (struct-out adapter)
         complete)

;; A tool offered to the model. `parameters` is its input JSON Schema (a jsexpr), as paisho
;; returns it.
(struct tool (name description parameters) #:transparent)

;; A tool call the model made. `arguments` is a jsexpr hash, or a string holding the raw
;; arguments if the model's arguments weren't valid JSON.
(struct tool-call (id name arguments) #:transparent)

(struct user-message (text) #:transparent)

;; `text` is #f when the model only called tools; `tool-calls` is a list, possibly empty.
(struct assistant-message (text tool-calls) #:transparent)

;; The result of the tool call `call-id`: its text, or the error text if `error?`.
(struct tool-message (call-id name text error?) #:transparent)

;; A model behind some provider. `complete-proc` takes the system prompt, the conversation
;; (a list of the message structs above, oldest first) and the tools, and returns the
;; model's reply as an `assistant-message`.
(struct adapter (name complete-proc))

(define (complete adapter system messages tools)
  ((adapter-complete-proc adapter) system messages tools))
