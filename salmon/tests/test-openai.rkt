#lang racket/base

(require json
         racket/list
         racket/string
         rackunit
         "../llm.rkt"
         "../openai.rkt"
         "fake-openai.rkt")

(define get-webpage
  (tool "get_webpage"
        "Returns the content of the webpage at a given URL."
        (hasheq 'type "object"
                'properties (hasheq 'url (hasheq 'type "string"))
                'required '("url"))))

(test-case "the request wraps tools and sends the conversation in Chat Completions form"
  (define call (tool-call "call_1" "get_webpage" (hasheq 'url "www.informations.com")))
  (define body
    (request-body "gpt-x"
                  "Be helpful."
                  (list (user-message "Read the page.")
                        (assistant-message #f (list call))
                        (tool-message "call_1" "get_webpage" "The page." #f))
                  (list get-webpage)))
  (check-equal? (hash-ref body 'model) "gpt-x")
  (check-equal? (hash-ref body 'temperature) 0)
  (check-equal? (hash-ref body 'tool_choice) "auto")
  (check-equal? (hash-ref body 'tools)
                (list (hasheq 'type "function"
                              'function (hasheq 'name "get_webpage"
                                                'description "Returns the content of the webpage at a given URL."
                                                'parameters (tool-parameters get-webpage)))))
  (define messages (hash-ref body 'messages))
  (check-equal? (map (lambda (m) (hash-ref m 'role)) messages) '("system" "user" "assistant" "tool"))
  (check-equal? (hash-ref (first messages) 'content) "Be helpful.")
  (define assistant (third messages))
  (check-equal? (hash-ref assistant 'content) (json-null))
  (check-equal? (hash-ref (first (hash-ref assistant 'tool_calls)) 'function)
                (hasheq 'name "get_webpage" 'arguments "{\"url\":\"www.informations.com\"}"))
  (check-equal? (fourth messages)
                (hasheq 'role "tool" 'tool_call_id "call_1" 'content "The page.")))

(test-case "temperature and tools are left out when not wanted"
  (define body (request-body "gpt-x" "S." (list (user-message "Hi.")) '() #:temperature #f))
  (check-false (hash-has-key? body 'temperature))
  (check-false (hash-has-key? body 'tools))
  (check-false (hash-has-key? body 'tool_choice)))

(test-case "replies are parsed into text and tool calls"
  (define reply
    (parse-reply (chat-reply #f
                             (list "a" "get_webpage" (hasheq 'url "x"))
                             (list "b" "send_direct_message" "{not json")
                             (list "c" "get_channels" ""))))
  (check-false (assistant-message-text reply))
  (check-equal? (assistant-message-tool-calls reply)
                (list (tool-call "a" "get_webpage" (hasheq 'url "x"))
                      ;; Malformed arguments are kept as written, for the agent to report.
                      (tool-call "b" "send_direct_message" "{not json")
                      ;; No arguments at all means none.
                      (tool-call "c" "get_channels" (hasheq))))
  (check-equal? (parse-reply (chat-reply "Done.")) (assistant-message "Done." '()))
  (check-equal? (parse-reply (chat-reply "")) (assistant-message #f '())))

(test-case "the adapter posts to the endpoint with the key, and retries rate limits"
  (define attempts 0)
  (define server
    (start-fake-openai (lambda (_body)
                         (set! attempts (add1 attempts))
                         (if (= attempts 1)
                             (values 429 (hasheq 'error (hasheq 'message "slow down")))
                             (values 200 (chat-reply "Hello."))))))
  (define model (openai-adapter "gpt-x" #:api-key "sk-test" #:base-url (fake-openai-base-url server)))
  (define reply
    (parameterize ([openai-initial-backoff 0])
      (complete model "S." (list (user-message "Hi.")) '())))
  (stop-fake-openai server)
  (check-equal? reply (assistant-message "Hello." '()))
  (check-equal? attempts 2)
  (define-values (headers body) (values (car (first (fake-openai-requests server)))
                                        (cdr (first (fake-openai-requests server)))))
  (check-not-false (member "Authorization: Bearer sk-test" headers))
  (check-equal? (hash-ref body 'model) "gpt-x"))

(test-case "other HTTP errors fail without retrying"
  (define server
    (start-fake-openai (lambda (_body) (values 400 (hasheq 'error (hasheq 'message "bad request"))))))
  (define model (openai-adapter "gpt-x" #:api-key "sk-test" #:base-url (fake-openai-base-url server)))
  (check-exn #rx"HTTP 400.*bad request"
             (lambda () (complete model "S." (list (user-message "Hi.")) '())))
  (stop-fake-openai server)
  (check-equal? (length (fake-openai-requests server)) 1))
