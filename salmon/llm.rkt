#lang racket/base

;; A provider-neutral conversation and the interface every model adapter implements. Adapters
;; convert to and from their provider's format in one place; the agent loop only sees these.

(require (only-in json json-null))

(provide (struct-out tool)
         (struct-out tool-call)
         (struct-out user-message)
         (struct-out assistant-message)
         (struct-out tool-message)
         (struct-out usage)
         no-usage
         usage+
         usage->jsexpr
         message->jsexpr
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

;; The tokens model calls used, as the provider reported them. `cached-input` is the part of
;; `input` the provider served from its prompt cache (billed at a lower rate by most), and
;; `calls` is how many calls the counts cover. A call whose provider reported no usage
;; counts in `calls` but adds no tokens.
(struct usage (calls input cached-input output) #:transparent)

(define no-usage (usage 0 0 0 0))

(define (usage+ a b)
  (usage (+ (usage-calls a) (usage-calls b))
         (+ (usage-input a) (usage-input b))
         (+ (usage-cached-input a) (usage-cached-input b))
         (+ (usage-output a) (usage-output b))))


;; `message` (one of the message structs above) as a jsexpr, for logs. Tool call arguments
;; are kept as parsed, or as the raw string the model wrote if they weren't valid JSON.
(define (message->jsexpr message)
  (cond
    [(user-message? message)
     (hasheq 'role "user" 'text (user-message-text message))]
    [(assistant-message? message)
     (hasheq 'role "assistant"
             'text (or (assistant-message-text message) (json-null))
             'tool_calls (for/list ([call (in-list (assistant-message-tool-calls message))])
                           (hasheq 'id (tool-call-id call)
                                   'name (tool-call-name call)
                                   'arguments (tool-call-arguments call))))]
    [(tool-message? message)
     (hasheq 'role "tool"
             'call_id (tool-message-call-id message)
             'name (tool-message-name message)
             'text (tool-message-text message)
             'error (tool-message-error? message))]))

(define (usage->jsexpr u)
  (hasheq 'calls (usage-calls u)
          'input_tokens (usage-input u)
          'cached_input_tokens (usage-cached-input u)
          'output_tokens (usage-output u)))

;; A model behind some provider. `complete-proc` takes the system prompt, the conversation
;; (a list of the message structs above, oldest first) and the tools, and returns two
;; values: the model's reply as an `assistant-message`, and the call's `usage`.
(struct adapter (name complete-proc))

(define (complete adapter system messages tools)
  ((adapter-complete-proc adapter) system messages tools))
