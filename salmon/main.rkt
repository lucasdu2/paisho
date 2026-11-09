#lang racket/base

;; salmon: an agent harness that runs against a paisho server.
;;
;;   racket main.rkt --model gpt-4o-2024-08-06 --suite slack \
;;       --attack important_instructions --workers 8

(require json
         racket/async-channel
         racket/cmdline
         racket/file
         racket/hash
         racket/list
         racket/string
         "agent.rkt"
         "llm.rkt"
         "openai.rkt"
         "paisho.rkt")

(provide run-benchmark)

;; Starts (or resumes) the benchmark `name` and runs its pending episodes that `select?`
;; accepts, on `workers` connections running in parallel. `model` is an adapter (see
;; llm.rkt). Returns the finished episodes (each with its `utility`, `security` and the
;; `usage` jsexpr of its model calls) and the failed ones (with why; they have no saved
;; result, so they stay pending).
;;
;; With `usage-log`, a file path, each episode run (finished or failed, since a failed one's
;; calls are paid for too) appends a JSON line to it with the episode, its result or error,
;; and the tokens its model calls used. Resumed runs append to the same file, so it covers
;; every episode run under `name`.
(define (run-benchmark model
                       #:name name
                       #:usage-log [usage-log #f]
                       #:suites suites
                       #:attacks attacks
                       #:model-name [model-name #f]
                       #:socket [socket "paisho.sock"]
                       #:workers [workers 1]
                       #:select? [select? (lambda (_) #t)]
                       #:log [log (lambda (line) (displayln line (current-error-port)))])
  (define conn (paisho-connect socket))
  (define started
    (paisho-request conn (let ([request (hasheq 'type "start_benchmark"
                                                'name name
                                                'suites suites
                                                'attacks attacks)])
                           (if model-name (hash-set request 'model_name model-name) request))))
  (paisho-close conn)
  (define pending (hash-ref started 'pending))
  (define episodes (filter select? pending))
  (log (format "~a: ~a; ~a of ~a episodes pending, running ~a"
               name
               (if (hash-ref started 'resumed) "resuming" "starting")
               (length pending)
               (hash-ref started 'total)
               (length episodes)))

  (define todo (make-async-channel))
  (for ([episode (in-list episodes)]) (async-channel-put todo episode))
  (define lock (make-semaphore 1))
  (define finished '())
  (define failed '())
  (define total no-usage)
  (when usage-log
    (define dir (let-values ([(dir _name _dir?) (split-path (path->complete-path usage-log))]) dir))
    (make-directory* dir))

  ;; Called with `lock` held.
  (define (record! episode episode-usage outcome)
    (set! total (usage+ total episode-usage))
    (when usage-log
      (with-output-to-file usage-log #:exists 'append
        (lambda ()
          (write-json (hash-union (hasheq 'benchmark name 'model (adapter-name model))
                                  episode
                                  outcome
                                  (usage->jsexpr episode-usage)))
          (newline)))))

  (define (worker)
    (define conn (paisho-connect socket))
    (let loop ()
      (define episode (sync/timeout 0 todo))
      (when episode
        ;; Kept outside the handler, so a failed episode still reports the calls it made.
        (define episode-usage no-usage)
        (define (add-usage! u) (set! episode-usage (usage+ episode-usage u)))
        (with-handlers ([exn:fail?
                         (lambda (e)
                           (call-with-semaphore
                            lock
                            (lambda ()
                              (set! failed (cons (cons episode (exn-message e)) failed))
                              (record! episode episode-usage (hasheq 'error (exn-message e)))))
                           (log (format "failed ~a: ~a" episode (exn-message e))))])
          (define-values (result _conversation)
            (run-episode model conn episode #:benchmark name #:on-usage add-usage!))
          (define outcome (hasheq 'utility (hash-ref result 'utility)
                                  'security (hash-ref result 'security)))
          (call-with-semaphore
           lock
           (lambda ()
             (set! finished (cons (hash-set* (hash-union episode outcome)
                                             'usage (usage->jsexpr episode-usage))
                                  finished))
             (record! episode episode-usage outcome))))
        (loop)))
    (paisho-close conn))

  ;; Racket threads are lightweight and interleave while waiting on the model and paisho,
  ;; which is where episodes spend their time.
  (for-each thread-wait (for/list ([_ (in-range (max 1 workers))]) (thread worker)))
  (log (format "~a: finished ~a, failed ~a" name (length finished) (length failed)))
  (log (format "~a: ~a model calls used ~a input tokens (~a cached) and ~a output tokens"
               name (usage-calls total) (usage-input total) (usage-cached-input total) (usage-output total)))
  (values (reverse finished) (reverse failed)))

;; The benchmark name for `model`, made safe as a directory name.
(define (default-benchmark-name model)
  (regexp-replace* #rx"[^A-Za-z0-9._-]" model "_"))

(module+ main
  (define model #f)
  (define base-url "https://api.openai.com/v1")
  (define api-key-variable "OPENAI_API_KEY")
  (define temperature 0)
  (define suites '())
  (define attacks '())
  (define tasks '())
  (define name #f)
  (define model-name #f)
  (define socket "paisho.sock")
  (define workers 1)
  (define usage-log 'default)
  (command-line
   #:program "salmon"
   #:once-each
   [("--model") m "Model, as the provider names it (required)" (set! model m)]
   [("--base-url") u "OpenAI-compatible API base URL (default: OpenAI's)" (set! base-url u)]
   [("--api-key-env") v "Environment variable holding the API key (default: OPENAI_API_KEY)"
                      (set! api-key-variable v)]
   [("--temperature") t "Sampling temperature (default: 0)" (set! temperature (string->number t))]
   [("--no-temperature") "Don't send a temperature (for models that reject one)" (set! temperature #f)]
   [("--benchmark") n "Benchmark name (default: the model)" (set! name n)]
   [("--model-name") n "How attacks address the model (default: paisho's generic name)" (set! model-name n)]
   [("--socket") s "paisho's socket name (default: paisho.sock)" (set! socket s)]
   [("--workers") n "Episodes run in parallel (default: 1)" (set! workers (string->number n))]
   [("--usage-log") f "Append each episode's token usage to this JSONL file (default: usage/<benchmark>.jsonl)"
                    (set! usage-log f)]
   [("--no-usage-log") "Don't write a usage log" (set! usage-log #f)]
   #:multi
   [("--suite") s "Suite to run (repeatable; at least one)" (set! suites (append suites (list s)))]
   [("--attack") a "Attack to run (repeatable)" (set! attacks (append attacks (list a)))]
   [("--task") t "Only run this task (repeatable)" (set! tasks (append tasks (list t)))])
  (unless model (raise-user-error 'salmon "--model is required"))
  (when (null? suites) (raise-user-error 'salmon "at least one --suite is required"))
  (define api-key
    (or (getenv api-key-variable)
        (raise-user-error 'salmon "the environment variable ~a isn't set" api-key-variable)))
  (define benchmark (or name (default-benchmark-name model)))
  (define-values (_finished failed)
    (run-benchmark (openai-adapter model #:api-key api-key #:base-url base-url #:temperature temperature)
                   #:name benchmark
                   #:usage-log (if (eq? usage-log 'default)
                                   (build-path "usage" (string-append benchmark ".jsonl"))
                                   usage-log)
                   #:suites suites
                   #:attacks attacks
                   #:model-name model-name
                   #:socket socket
                   #:workers workers
                   #:select? (if (null? tasks)
                                 (lambda (_) #t)
                                 (lambda (episode) (and (member (hash-ref episode 'task) tasks) #t)))))
  (exit (if (null? failed) 0 1)))
