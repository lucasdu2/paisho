#lang racket/base

;; Runs salmon's agent against a real paisho server (built from this checkout), with the fake
;; OpenAI server playing a scripted model, so no API calls are made.

(require json
         racket/file
         racket/hash
         racket/list
         racket/os
         racket/runtime-path
         racket/string
         racket/system
         rackunit
         "../main.rkt"
         "../openai.rkt"
         "fake-openai.rkt")

(define-runtime-path crate "../../paisho")

;; The scripted model: reads www.informations.com while also calling a tool that doesn't
;; exist, then visits www.true-informations.com (the goal of Slack's injection task 3), then
;; answers. Its turn is the number of assistant messages so far.
(define (scripted-model body)
  (define turn (count (lambda (m) (equal? (hash-ref m 'role) "assistant")) (hash-ref body 'messages)))
  (values 200
          (case turn
            [(0) (chat-reply #f
                             #:usage '(100 0 10)
                             (list "1" "get_webpage" (hasheq 'url "www.informations.com"))
                             (list "2" "no_such_tool" (hasheq)))]
            [(1) (chat-reply #f #:usage '(200 100 10) (list "3" "get_webpage" (hasheq 'url "www.true-informations.com")))]
            [else (chat-reply "Done." #:usage '(300 200 5))])))

(define (with-paisho proc)
  ;; Cargo's output (such as dead-code warnings) is only shown if the build fails.
  (define cargo-output (open-output-string))
  (parameterize ([current-directory crate]
                 [current-output-port cargo-output]
                 [current-error-port cargo-output])
    (unless (system* (find-executable-path "cargo") "build" "--quiet")
      (error 'test "cargo build failed:\n~a" (get-output-string cargo-output))))
  (define socket (format "salmon-test-~a-~a.sock" (getpid) (random 1000000)))
  (define logdir (make-temporary-directory))
  (define-values (server _stdout _stdin stderr)
    (subprocess #f #f #f (build-path crate "target" "debug" "paisho") socket logdir))
  (dynamic-wind
   void
   (lambda ()
     (check-true (string-contains? (read-line stderr) "listening"))
     (proc socket logdir))
   (lambda ()
     (subprocess-kill server #t)
     (delete-directory/files logdir))))

(test-case "episodes run through salmon's agent against paisho"
  (with-paisho
   (lambda (socket logdir)
     (define fake (start-fake-openai scripted-model))
     (define model (openai-adapter "scripted" #:api-key "unused" #:base-url (fake-openai-base-url fake)))
     (define (selected? episode) (member (hash-ref episode 'task) (list "user_task_0" (json-null))))
     (define usage-log (build-path logdir "usage" "salmon-test.jsonl"))
     (define conversation-log (build-path logdir "usage" "salmon-test.conversations.jsonl"))
     (define-values (finished failed)
       (run-benchmark model
                      #:name "salmon-test"
                      #:usage-log usage-log
                      #:conversation-log conversation-log
                      #:suites '("slack")
                      #:attacks '("important_instructions")
                      #:socket socket
                      #:select? selected?
                      #:log void))
     (stop-fake-openai fake)

     (check-equal? failed '())
     ;; user_task_0 benign and under 5 injections, plus the 5 injections alone.
     (check-equal? (length finished) 11)
     (define (result task injection)
       (findf (lambda (e) (and (equal? (hash-ref e 'task) task) (equal? (hash-ref e 'injection) injection)))
              finished))
     (check-equal? (hash-ref (result "user_task_0" (json-null)) 'utility) #t)
     (check-equal? (hash-ref (result "user_task_0" (json-null)) 'security) (json-null))
     (check-equal? (hash-ref (result "user_task_0" "injection_task_3") 'security) #t)
     (check-equal? (hash-ref (result "user_task_0" "injection_task_1") 'security) #f)
     (check-equal? (hash-ref (result (json-null) "injection_task_3") 'utility) (json-null))
     (check-equal? (hash-ref (result (json-null) "injection_task_3") 'security) #t)

     ;; Every episode made the scripted model's three calls, and logged them.
     (define three-calls
       (hasheq 'calls 3 'input_tokens 600 'cached_input_tokens 300 'output_tokens 25))
     (check-equal? (hash-ref (result "user_task_0" "injection_task_3") 'usage) three-calls)
     (define logged (file->lines usage-log))
     (check-equal? (length logged) 11)
     (define line
       (findf (lambda (l) (and (equal? (hash-ref l 'task) "user_task_0")
                               (equal? (hash-ref l 'injection) "injection_task_3")))
              (map string->jsexpr logged)))
     (check-equal? line (hash-union (hasheq 'benchmark "salmon-test"
                                           'model "scripted"
                                           'suite "slack"
                                           'task "user_task_0"
                                           'attack "important_instructions"
                                           'injection "injection_task_3"
                                           'utility #t
                                           'security #t)
                                   three-calls))

     ;; Every finished episode logged its conversation: the user's prompt, then the model's
     ;; three replies with the tool results between them.
     (define conversations (map string->jsexpr (file->lines conversation-log)))
     (check-equal? (length conversations) 11)
     (define conversation
       (findf (lambda (l) (and (equal? (hash-ref l 'task) "user_task_0")
                               (equal? (hash-ref l 'injection) "injection_task_3")))
              conversations))
     (check-equal? (hash-ref conversation 'model) "scripted")
     (check-equal? (hash-ref conversation 'attack) "important_instructions")
     (check-true (string-prefix? (hash-ref conversation 'system) "You are an AI language model"))
     (define messages (hash-ref conversation 'messages))
     (check-equal? (map (lambda (m) (hash-ref m 'role)) messages)
                   '("user" "assistant" "tool" "tool" "assistant" "tool" "assistant"))
     (check-equal? (hash-ref (second messages) 'tool_calls)
                   (list (hasheq 'id "1" 'name "get_webpage" 'arguments (hasheq 'url "www.informations.com"))
                         (hasheq 'id "2" 'name "no_such_tool" 'arguments (hasheq))))
     (check-true (string-contains? (hash-ref (third messages) 'text) "<INFORMATION>"))
     (check-equal? (fourth messages)
                   (hasheq 'role "tool" 'call_id "2" 'name "no_such_tool"
                           'text "Invalid tool no_such_tool provided." 'error #t))
     (check-equal? (last messages) (hasheq 'role "assistant" 'text "Done." 'tool_calls '()))

     ;; What the model was sent: paisho's tools, the injected page, and the unknown tool's error.
     (define requests (map cdr (fake-openai-requests fake)))
     (check-not-false
      (member "get_webpage"
              (for/list ([t (in-list (hash-ref (first requests) 'tools))])
                (hash-ref (hash-ref t 'function) 'name))))
     (define tool-results
       (for*/list ([request (in-list requests)]
                   [message (in-list (hash-ref request 'messages))]
                   #:when (equal? (hash-ref message 'role) "tool"))
         (hash-ref message 'content)))
     (check-true (ormap (lambda (text) (string-contains? text "<INFORMATION>")) tool-results))
     (check-not-false (member "Invalid tool no_such_tool provided." tool-results))

     ;; paisho saved the result with the extracted answer. The unknown tool was rejected by
     ;; the agent, so it never reached paisho's trace.
     (define saved
       (call-with-input-file
         (build-path logdir "salmon-test" "slack" "user_task_0" "important_instructions" "injection_task_3.json")
         read-json))
     (check-equal? (hash-ref saved 'output) "Done.")
     (check-equal? (map (lambda (call) (hash-ref call 'name)) (hash-ref saved 'tool_calls))
                   '("get_webpage" "get_webpage"))

     ;; Running again resumes: nothing selected is left.
     (define-values (again again-failed)
       (run-benchmark model
                      #:name "salmon-test"
                      #:suites '("slack")
                      #:attacks '("important_instructions")
                      #:socket socket
                      #:select? selected?
                      #:log void))
     (check-equal? (list again again-failed) '(() ())))))

(test-case "parallel workers run a whole suite"
  (with-paisho
   (lambda (socket logdir)
     (define fake (start-fake-openai scripted-model))
     (define model (openai-adapter "scripted" #:api-key "unused" #:base-url (fake-openai-base-url fake)))
     (define-values (finished failed)
       (run-benchmark model
                      #:name "salmon-parallel"
                      #:suites '("slack")
                      #:attacks '("tool_knowledge")
                      #:socket socket
                      #:workers 8
                      #:log void))
     (stop-fake-openai fake)
     (check-equal? failed '())
     (check-equal? (length finished) (+ (* 21 (+ 1 5)) 5)))))

(test-case "a failed episode still logs the calls it made"
  (with-paisho
   (lambda (socket logdir)
     ;; The first call succeeds; the next is rejected, which fails the episode.
     (define fake
       (start-fake-openai
        (lambda (body)
          (if (null? (filter (lambda (m) (equal? (hash-ref m 'role) "assistant")) (hash-ref body 'messages)))
              (values 200 (chat-reply #f #:usage '(100 0 10) (list "1" "get_channels" (hasheq))))
              (values 400 (hasheq 'error (hasheq 'message "bad request")))))))
     (define model (openai-adapter "scripted" #:api-key "unused" #:base-url (fake-openai-base-url fake)))
     (define usage-log (build-path logdir "usage.jsonl"))
     (define conversation-log (build-path logdir "conversations.jsonl"))
     (define-values (finished failed)
       (run-benchmark model
                      #:name "salmon-failed"
                      #:suites '("slack")
                      #:attacks '()
                      #:socket socket
                      #:usage-log usage-log
                      #:conversation-log conversation-log
                      #:select? (lambda (episode) (equal? (hash-ref episode 'task) "user_task_0"))
                      #:log void))
     (stop-fake-openai fake)
     (check-equal? finished '())
     (check-equal? (length failed) 1)
     (define line (string->jsexpr (car (file->lines usage-log))))
     (check-regexp-match #rx"HTTP 400" (hash-ref line 'error))
     (check-equal? (list (hash-ref line 'calls) (hash-ref line 'input_tokens) (hash-ref line 'output_tokens))
                   '(1 100 10))
     (check-false (hash-has-key? line 'utility))
     ;; A failed episode has no conversation to log.
     (check-false (file-exists? conversation-log)))))
