#lang racket/base

;; A model adapter for OpenAI's Chat Completions API, using its native tool calling. The base
;; URL is configurable, so the same adapter works with OpenAI-compatible services such as
;; OpenRouter or a local vLLM server.

(require json
         net/http-client
         net/url
         racket/list
         racket/string
         "llm.rkt")

(provide openai-adapter
         openai-initial-backoff
         ;; Exposed for tests.
         request-body
         parse-reply)

;; Which statuses and how many times to retry, with exponential backoff starting at
;; `(openai-initial-backoff)` seconds: rate limits and server errors.
(define retryable-statuses '(408 409 429 500 502 503 504))
(define max-attempts 5)
(define openai-initial-backoff (make-parameter 1))

;; An adapter for `model`. `temperature` defaults to 0, as in AgentDojo; pass #f to leave it
;; out, for models that reject it (such as OpenAI's reasoning models). `system-role` is
;; "system", which every OpenAI-compatible service accepts; OpenAI also accepts "developer",
;; which AgentDojo uses.
(define (openai-adapter model
                        #:api-key api-key
                        #:base-url [base-url "https://api.openai.com/v1"]
                        #:temperature [temperature 0]
                        #:system-role [system-role "system"])
  (define endpoint (string->url (string-append (string-trim base-url "/" #:left? #f) "/chat/completions")))
  (adapter
   model
   (lambda (system messages tools)
     (parse-reply
      (post-json endpoint
                 api-key
                 (request-body model system messages tools
                               #:temperature temperature
                               #:system-role system-role))))))

;; The Chat Completions request for the conversation.
(define (request-body model system messages tools
                      #:temperature [temperature 0]
                      #:system-role [system-role "system"])
  (define body
    (hasheq 'model model
            'messages (cons (hasheq 'role system-role 'content system)
                            (map message->json messages))))
  (let* ([body (if temperature (hash-set body 'temperature temperature) body)]
         [body (if (null? tools)
                   body
                   (hash-set* body
                              'tools (map tool->json tools)
                              'tool_choice "auto"))])
    body))

(define (tool->json t)
  (hasheq 'type "function"
          'function (hasheq 'name (tool-name t)
                            'description (tool-description t)
                            'parameters (tool-parameters t))))

(define (message->json message)
  (cond
    [(user-message? message)
     (hasheq 'role "user" 'content (user-message-text message))]
    [(assistant-message? message)
     (define calls (assistant-message-tool-calls message))
     (define base
       (hasheq 'role "assistant"
               'content (or (assistant-message-text message) (json-null))))
     (if (null? calls)
         base
         (hash-set base 'tool_calls (map tool-call->json calls)))]
    [(tool-message? message)
     ;; Chat Completions has no error flag for tool results; as in AgentDojo's adapter, an
     ;; error is sent as the result's content.
     (hasheq 'role "tool"
             'tool_call_id (tool-message-call-id message)
             'content (tool-message-text message))]))

(define (tool-call->json call)
  (define arguments (tool-call-arguments call))
  (hasheq 'id (tool-call-id call)
          'type "function"
          'function (hasheq 'name (tool-call-name call)
                            ;; Sent back as the model wrote it, even if it wasn't valid JSON.
                            'arguments (if (string? arguments) arguments (jsexpr->string arguments)))))

;; The model's reply in a Chat Completions response.
(define (parse-reply response)
  (define choices (hash-ref response 'choices '()))
  (when (null? choices)
    (error 'openai "response has no choices: ~a" (jsexpr->string response)))
  (define message (hash-ref (first choices) 'message))
  (define content (hash-ref message 'content (json-null)))
  (define calls (let ([calls (hash-ref message 'tool_calls (json-null))])
                  (if (eq? calls (json-null)) '() calls)))
  (assistant-message
   (and (string? content) (not (string=? content "")) content)
   (for/list ([call (in-list calls)])
     (define function (hash-ref call 'function))
     (define raw (hash-ref function 'arguments ""))
     (tool-call (hash-ref call 'id)
                (hash-ref function 'name)
                ;; Malformed arguments are kept as the raw string, so the agent loop can
                ;; report the problem to the model instead of failing the episode.
                (with-handlers ([exn:fail? (lambda (_) raw)])
                  (let ([parsed (if (string=? (string-trim raw) "") (hasheq) (string->jsexpr raw))])
                    (if (hash? parsed) parsed raw)))))))

;; POSTs `body` to `endpoint` and returns the response's JSON, retrying rate limits, server
;; errors and connection failures.
(define (post-json endpoint api-key body)
  (define ssl? (equal? (url-scheme endpoint) "https"))
  (define host (url-host endpoint))
  (define port (or (url-port endpoint) (if ssl? 443 80)))
  (define uri (string-append "/" (string-join (map path/param-path (url-path endpoint)) "/")))
  (define headers
    (list "Content-Type: application/json"
          (string-append "Authorization: Bearer " api-key)))
  (define data (jsexpr->bytes body))
  (let loop ([attempt 1])
    (define (retry why)
      (if (< attempt max-attempts)
          (begin
            (sleep (* (openai-initial-backoff) (expt 2 (sub1 attempt))))
            (loop (add1 attempt)))
          (error 'openai "giving up after ~a attempts: ~a" attempt why)))
    (define result
      (with-handlers ([exn:fail:network? (lambda (e) e)])
        (define-values (status-line _headers in)
          (http-sendrecv host uri #:ssl? ssl? #:port port #:method "POST" #:headers headers #:data data))
        (define status (string->number (cadr (string-split (bytes->string/utf-8 status-line)))))
        (define text (bytes->string/utf-8 (read-all-bytes in) #\?))
        (close-input-port in)
        (cons status text)))
    (cond
      [(exn? result) (retry (exn-message result))]
      [(= (car result) 200) (string->jsexpr (cdr result))]
      [(memv (car result) retryable-statuses) (retry (format "HTTP ~a: ~a" (car result) (cdr result)))]
      [else (error 'openai "HTTP ~a: ~a" (car result) (cdr result))])))

(define (read-all-bytes in)
  (let loop ([chunks '()])
    (define chunk (read-bytes 65536 in))
    (if (eof-object? chunk)
        (apply bytes-append (reverse chunks))
        (loop (cons chunk chunks)))))
