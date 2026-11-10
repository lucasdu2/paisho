#lang racket/base

;; A model adapter for OpenAI's Responses API, using its native tool calling. Some models
;; (such as gpt-6-luna) only take tools with their default reasoning through this API, not
;; through Chat Completions.
;;
;; Requests are stateless (`store` is false): each sends the whole conversation, as the Chat
;; Completions adapter does. A reasoning model's reasoning items are sent back on later turns,
;; as OpenAI recommends for tool calling, in the encrypted form it returns for that.

(require json
         net/url
         racket/list
         racket/string
         "llm.rkt"
         (only-in "openai.rkt" post-json parse-arguments))

(provide responses-adapter
         ;; Exposed for tests.
         (struct-out responses-message)
         request-body
         parse-reply
         parse-usage)

;; An assistant message from the Responses API, which keeps the response's output `items`
;; (jsexprs, oldest first) to send back as they were, reasoning items included.
(struct responses-message assistant-message (items) #:transparent)

;; An adapter for `model`. `temperature` is left out by default, since reasoning models
;; reject it; `reasoning-effort` too, leaving the model's default. `system-role` is as in
;; `openai-adapter`.
(define (responses-adapter model
                           #:api-key api-key
                           #:base-url [base-url "https://api.openai.com/v1"]
                           #:temperature [temperature #f]
                           #:reasoning-effort [reasoning-effort #f]
                           #:system-role [system-role "system"])
  (define endpoint (string->url (string-append (string-trim base-url "/" #:left? #f) "/responses")))
  (adapter
   model
   (lambda (system messages tools)
     (define response
       (post-json endpoint
                  api-key
                  (request-body model system messages tools
                                #:temperature temperature
                                #:reasoning-effort reasoning-effort
                                #:system-role system-role)))
     (values (parse-reply response) (parse-usage response)))))

;; The Responses request for the conversation.
(define (request-body model system messages tools
                      #:temperature [temperature #f]
                      #:reasoning-effort [reasoning-effort #f]
                      #:system-role [system-role "system"])
  (define body
    (hasheq 'model model
            'input (cons (hasheq 'role system-role 'content system)
                         (append-map message->items messages))
            'store #f
            'include '("reasoning.encrypted_content")))
  (let* ([body (if temperature (hash-set body 'temperature temperature) body)]
         [body (if reasoning-effort
                   (hash-set body 'reasoning (hasheq 'effort reasoning-effort))
                   body)]
         [body (if (null? tools)
                   body
                   (hash-set* body
                              'tools (map tool->json tools)
                              'tool_choice "auto"))])
    body))

(define (tool->json t)
  (hasheq 'type "function"
          'name (tool-name t)
          'description (tool-description t)
          'parameters (tool-parameters t)
          ;; The Responses API makes function tools strict unless told otherwise, which
          ;; paisho's schemas (with optional parameters) don't satisfy. Chat Completions
          ;; doesn't, so neither does this.
          'strict #f))

;; The input items for `message`.
(define (message->items message)
  (cond
    [(user-message? message)
     (list (hasheq 'role "user" 'content (user-message-text message)))]
    [(responses-message? message) (responses-message-items message)]
    [(assistant-message? message)
     (define text (assistant-message-text message))
     (append (if text (list (hasheq 'role "assistant" 'content text)) '())
             (map tool-call->item (assistant-message-tool-calls message)))]
    [(tool-message? message)
     ;; Function call outputs have no error flag; as in the Chat Completions adapter, an
     ;; error is sent as the output.
     (list (hasheq 'type "function_call_output"
                   'call_id (tool-message-call-id message)
                   'output (tool-message-text message)))]))

(define (tool-call->item call)
  (define arguments (tool-call-arguments call))
  (hasheq 'type "function_call"
          'call_id (tool-call-id call)
          'name (tool-call-name call)
          'arguments (if (string? arguments) arguments (jsexpr->string arguments))))

;; The model's reply in a Responses response.
(define (parse-reply response)
  (define items (hash-ref response 'output (json-null)))
  (unless (list? items)
    (error 'responses "response has no output: ~a" (jsexpr->string response)))
  (define text
    (string-append*
     (for*/list ([item (in-list items)]
                 #:when (equal? (hash-ref item 'type #f) "message")
                 [part (in-list (hash-ref item 'content '()))]
                 #:when (equal? (hash-ref part 'type #f) "output_text"))
       (hash-ref part 'text ""))))
  (responses-message
   (and (not (string=? text "")) text)
   (for/list ([item (in-list items)]
              #:when (equal? (hash-ref item 'type #f) "function_call"))
     (tool-call (hash-ref item 'call_id)
                (hash-ref item 'name)
                (parse-arguments (hash-ref item 'arguments ""))))
   items))

;; The one call's usage in a Responses response, counted as `openai.rkt`'s `parse-usage`
;; counts Chat Completions'. Reasoning tokens are part of `output_tokens`.
(define (parse-usage response)
  (define (count table key)
    (define n (and (hash? table) (hash-ref table key #f)))
    (if (exact-nonnegative-integer? n) n 0))
  (define u (hash-ref response 'usage #f))
  (usage 1
         (count u 'input_tokens)
         (count (and (hash? u) (hash-ref u 'input_tokens_details #f)) 'cached_tokens)
         (count u 'output_tokens)))
