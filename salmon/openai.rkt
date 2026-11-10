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
         openai-max-retry-wait
         openai-sleep
         post-json
         parse-arguments
         ;; Exposed for tests.
         request-body
         parse-reply
         parse-usage)

;; Which statuses and how many times to retry, with exponential backoff starting at
;; `(openai-initial-backoff)` seconds: rate limits and server errors. A response that says
;; how long to wait (`retry-after-ms` or `retry-after`, as OpenAI sends on rate limits) is
;; waited for instead when that's longer, up to `(openai-max-retry-wait)` seconds, so a
;; per-minute limit doesn't use up the attempts. `openai-sleep` does the waiting; tests
;; replace it.
(define retryable-statuses '(408 409 429 500 502 503 504))
(define max-attempts 5)
(define openai-initial-backoff (make-parameter 1))
(define openai-max-retry-wait (make-parameter 60))
(define openai-sleep (make-parameter sleep))

;; An adapter for `model`. `temperature` defaults to 0, as in AgentDojo; pass #f to leave it
;; out, for models that reject it (such as OpenAI's reasoning models). `reasoning-effort` is
;; left out by default, leaving the model's; "none" turns reasoning off, which some models
;; (such as gpt-6-luna) need in order to take tools through this API. `system-role` is
;; "system", which every OpenAI-compatible service accepts; OpenAI also accepts "developer",
;; which AgentDojo uses.
(define (openai-adapter model
                        #:api-key api-key
                        #:base-url [base-url "https://api.openai.com/v1"]
                        #:temperature [temperature 0]
                        #:reasoning-effort [reasoning-effort #f]
                        #:system-role [system-role "system"])
  (define endpoint (string->url (string-append (string-trim base-url "/" #:left? #f) "/chat/completions")))
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

;; The Chat Completions request for the conversation.
(define (request-body model system messages tools
                      #:temperature [temperature 0]
                      #:reasoning-effort [reasoning-effort #f]
                      #:system-role [system-role "system"])
  (define body
    (hasheq 'model model
            'messages (cons (hasheq 'role system-role 'content system)
                            (map message->json messages))))
  (let* ([body (if temperature (hash-set body 'temperature temperature) body)]
         [body (if reasoning-effort (hash-set body 'reasoning_effort reasoning-effort) body)]
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
                (parse-arguments raw)))))

;; A tool call's arguments as the model wrote them, parsed. Malformed arguments are kept as
;; the raw string, so the agent loop can report the problem to the model instead of failing
;; the episode.
(define (parse-arguments raw)
  (with-handlers ([exn:fail? (lambda (_) raw)])
    (let ([parsed (if (string=? (string-trim raw) "") (hasheq) (string->jsexpr raw))])
      (if (hash? parsed) parsed raw))))

;; The one call's usage in a Chat Completions response. Services that leave out `usage` (or
;; its cached-token details) count as having used no tokens. Reasoning tokens are part of
;; `completion_tokens`, so they count as output.
(define (parse-usage response)
  (define (count table key)
    (define n (and (hash? table) (hash-ref table key #f)))
    (if (exact-nonnegative-integer? n) n 0))
  (define u (hash-ref response 'usage #f))
  (usage 1
         (count u 'prompt_tokens)
         (count (and (hash? u) (hash-ref u 'prompt_tokens_details #f)) 'cached_tokens)
         (count u 'completion_tokens)))

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
    (define (retry why [requested #f])
      (if (< attempt max-attempts)
          (let ([backoff (* (openai-initial-backoff) (expt 2 (sub1 attempt)))])
            ((openai-sleep) (if requested
                                (min (max backoff requested) (openai-max-retry-wait))
                                backoff))
            (loop (add1 attempt)))
          (error 'openai "giving up after ~a attempts: ~a" attempt why)))
    (define result
      (with-handlers ([exn:fail:network? (lambda (e) e)])
        (define-values (status-line response-headers in)
          (http-sendrecv host uri #:ssl? ssl? #:port port #:method "POST" #:headers headers #:data data))
        (define status (string->number (cadr (string-split (bytes->string/utf-8 status-line)))))
        (define text (bytes->string/utf-8 (read-all-bytes in) #\?))
        (close-input-port in)
        (list status text response-headers)))
    (cond
      [(exn? result) (retry (exn-message result))]
      [else
       (define-values (status text response-headers) (apply values result))
       (cond
         [(= status 200) (string->jsexpr text)]
         [(memv status retryable-statuses)
          (retry (format "HTTP ~a: ~a" status text) (requested-wait response-headers))]
         [else (error 'openai "HTTP ~a: ~a" status text)])])))

;; The seconds `headers` (a response's, as bytes) ask the client to wait before retrying, or
;; #f: `retry-after-ms`, which OpenAI sends, or else `retry-after` in seconds. A
;; `retry-after` date is ignored, as are values that aren't non-negative numbers.
(define (requested-wait headers)
  (define (header name)
    (for/or ([line (in-list headers)])
      (define m (regexp-match #rx#"^([^:]*):[ \t]*(.*?)[ \t\r]*$" line))
      (and m
           (string-ci=? (bytes->string/utf-8 (cadr m) #\?) name)
           (let ([n (string->number (bytes->string/utf-8 (caddr m) #\?) 10)])
             (and (real? n) (>= n 0) n)))))
  (cond
    [(header "retry-after-ms") => (lambda (ms) (/ ms 1000.0))]
    [(header "retry-after") => exact->inexact]
    [else #f]))

(define (read-all-bytes in)
  (let loop ([chunks '()])
    (define chunk (read-bytes 65536 in))
    (if (eof-object? chunk)
        (apply bytes-append (reverse chunks))
        (loop (cons chunk chunks)))))
