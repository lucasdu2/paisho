#lang racket/base

(require json
         racket/list
         rackunit
         "../llm.rkt"
         (only-in "../openai.rkt" openai-initial-backoff)
         "../responses.rkt"
         "fake-openai.rkt")

(define get-webpage
  (tool "get_webpage"
        "Returns the content of the webpage at a given URL."
        (hasheq 'type "object"
                'properties (hasheq 'url (hasheq 'type "string"))
                'required '("url"))))

;; A Responses response with `items` as its output and, with `usage`, a list of input, cached
;; and output tokens.
(define (responses-reply #:usage [usage #f] . items)
  (define response (hasheq 'object "response" 'output items))
  (if usage
      (hash-set response 'usage (hasheq 'input_tokens (first usage)
                                        'input_tokens_details (hasheq 'cached_tokens (second usage))
                                        'output_tokens (third usage)
                                        'total_tokens (+ (first usage) (third usage))))
      response))

(define reasoning
  (hasheq 'type "reasoning" 'id "rs_1" 'summary '() 'encrypted_content "opaque"))

(define (function-call id name arguments)
  (hasheq 'type "function_call" 'id (string-append "fc_" id) 'call_id id 'name name
          'arguments arguments 'status "completed"))

(define (message text)
  (hasheq 'type "message" 'id "msg_1" 'role "assistant" 'status "completed"
          'content (list (hasheq 'type "output_text" 'text text 'annotations '()))))

(test-case "the request is stateless, asks for encrypted reasoning and sends non-strict tools"
  (define body (request-body "gpt-x" "Be helpful." (list (user-message "Read the page.")) (list get-webpage)))
  (check-equal? (hash-ref body 'model) "gpt-x")
  (check-equal? (hash-ref body 'store) #f)
  (check-equal? (hash-ref body 'include) '("reasoning.encrypted_content"))
  (check-equal? (hash-ref body 'tool_choice) "auto")
  (check-equal? (hash-ref body 'tools)
                (list (hasheq 'type "function"
                              'name "get_webpage"
                              'description "Returns the content of the webpage at a given URL."
                              'parameters (tool-parameters get-webpage)
                              'strict #f)))
  (check-equal? (hash-ref body 'input)
                (list (hasheq 'role "system" 'content "Be helpful.")
                      (hasheq 'role "user" 'content "Read the page.")))
  ;; Left to the model by default.
  (check-false (hash-has-key? body 'temperature))
  (check-false (hash-has-key? body 'reasoning)))

(test-case "temperature and reasoning effort are sent when given, and tools left out when none"
  (define body (request-body "gpt-x" "S." (list (user-message "Hi.")) '()
                             #:temperature 0 #:reasoning-effort "low"))
  (check-equal? (hash-ref body 'temperature) 0)
  (check-equal? (hash-ref body 'reasoning) (hasheq 'effort "low"))
  (check-false (hash-has-key? body 'tools))
  (check-false (hash-has-key? body 'tool_choice)))

(test-case "replies are parsed into text and tool calls, keeping their output items"
  (define items (list reasoning
                      (function-call "a" "get_webpage" "{\"url\":\"x\"}")
                      (function-call "b" "send_direct_message" "{not json")
                      (function-call "c" "get_channels" "")))
  (define reply (parse-reply (apply responses-reply items)))
  (check-false (assistant-message-text reply))
  (check-equal? (assistant-message-tool-calls reply)
                (list (tool-call "a" "get_webpage" (hasheq 'url "x"))
                      (tool-call "b" "send_direct_message" "{not json")
                      (tool-call "c" "get_channels" (hasheq))))
  (check-equal? (responses-message-items reply) items)
  (check-equal? (assistant-message-text (parse-reply (responses-reply (message "Done.")))) "Done.")
  (check-false (assistant-message-text (parse-reply (responses-reply (message "")))))
  (check-exn #rx"no output" (lambda () (parse-reply (hasheq 'error "bad")))))

(test-case "a reply's items, reasoning included, are sent back as they were"
  (define items (list reasoning (function-call "a" "get_webpage" "{\"url\":\"x\"}")))
  (define reply (parse-reply (apply responses-reply items)))
  (define body
    (request-body "gpt-x" "S."
                  (list (user-message "Read x.")
                        reply
                        (tool-message "a" "get_webpage" "The page." #f))
                  (list get-webpage)))
  (check-equal? (cddr (hash-ref body 'input))
                (append items
                        (list (hasheq 'type "function_call_output" 'call_id "a" 'output "The page.")))))

(test-case "assistant messages from elsewhere are sent as a message and function calls"
  (define body
    (request-body "gpt-x" "S."
                  (list (assistant-message "Reading." (list (tool-call "a" "get_webpage" (hasheq 'url "x")))))
                  '()))
  (check-equal? (cdr (hash-ref body 'input))
                (list (hasheq 'role "assistant" 'content "Reading.")
                      (hasheq 'type "function_call" 'call_id "a" 'name "get_webpage"
                              'arguments "{\"url\":\"x\"}"))))

(test-case "usage is read from the response, and missing usage counts as no tokens"
  (check-equal? (parse-usage (responses-reply (message "Hi.") #:usage '(1200 1024 35)))
                (usage 1 1200 1024 35))
  (check-equal? (parse-usage (responses-reply (message "Hi."))) (usage 1 0 0 0)))

(test-case "the adapter posts with the key, and retries rate limits"
  (define attempts 0)
  (define server
    (start-fake-openai (lambda (_body)
                         (set! attempts (add1 attempts))
                         (if (= attempts 1)
                             (values 429 (hasheq 'error (hasheq 'message "slow down")))
                             (values 200 (responses-reply (message "Hello.") #:usage '(20 0 3)))))))
  (define model (responses-adapter "gpt-x" #:api-key "sk-test" #:base-url (fake-openai-base-url server)))
  (define-values (reply reply-usage)
    (parameterize ([openai-initial-backoff 0])
      (complete model "S." (list (user-message "Hi.")) '())))
  (stop-fake-openai server)
  (check-equal? (assistant-message-text reply) "Hello.")
  (check-equal? reply-usage (usage 1 20 0 3))
  (check-equal? attempts 2)
  (define-values (headers body) (values (car (first (fake-openai-requests server)))
                                        (cdr (first (fake-openai-requests server)))))
  (check-not-false (member "Authorization: Bearer sk-test" headers))
  (check-equal? (hash-ref body 'model) "gpt-x"))
