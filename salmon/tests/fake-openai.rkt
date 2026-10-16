#lang racket/base

;; A fake OpenAI-compatible server for tests: plain HTTP on localhost, answering each POST
;; with whatever `respond` returns for its JSON body. No network access or API key needed.

(require json
         racket/string
         racket/tcp)

(provide start-fake-openai
         fake-openai-base-url
         fake-openai-requests
         stop-fake-openai
         chat-reply)

;; `requests` holds each request as (cons headers body-jsexpr), oldest first.
(struct fake-openai (listener [thread #:mutable] port [requests #:mutable]))

;; Starts a server that answers each request with `(respond body)`, which returns a status
;; code and a response jsexpr.
(define (start-fake-openai respond)
  (define listener (tcp-listen 0 16 #t "127.0.0.1"))
  (define-values (_host port _remote-host _remote-port) (tcp-addresses listener #t))
  (define server (fake-openai listener #f port '()))
  (define (serve)
    (define-values (in out) (tcp-accept listener))
    (define _request-line (read-line in 'return-linefeed))
    (define headers
      (let loop ([headers '()])
        (define line (read-line in 'return-linefeed))
        (if (or (eof-object? line) (string=? line "")) (reverse headers) (loop (cons line headers)))))
    (define length
      (for/or ([header (in-list headers)])
        (define parts (string-split header ":"))
        (and (string-ci=? (car parts) "content-length") (string->number (string-trim (cadr parts))))))
    (define body (bytes->jsexpr (read-bytes (or length 0) in)))
    (set-fake-openai-requests! server (append (fake-openai-requests server) (list (cons headers body))))
    (define-values (status response) (respond body))
    (define payload (jsexpr->bytes response))
    (fprintf out "HTTP/1.1 ~a Status\r\nContent-Type: application/json\r\nContent-Length: ~a\r\nConnection: close\r\n\r\n"
             status (bytes-length payload))
    (write-bytes payload out)
    (close-output-port out)
    (close-input-port in)
    (serve))
  (set-fake-openai-thread! server (thread serve))
  server)

(define (fake-openai-base-url server)
  (format "http://127.0.0.1:~a/v1" (fake-openai-port server)))

(define (stop-fake-openai server)
  (kill-thread (fake-openai-thread server))
  (tcp-close (fake-openai-listener server)))

;; A successful Chat Completions response with `content` (or #f) and `tool-calls`, each a
;; list of id, name and arguments (a jsexpr, or a string sent as is).
(define (chat-reply content . tool-calls)
  (define message
    (hasheq 'role "assistant"
            'content (or content (json-null))
            'tool_calls (if (null? tool-calls)
                            (json-null)
                            (for/list ([call (in-list tool-calls)])
                              (hasheq 'id (car call)
                                      'type "function"
                                      'function (hasheq 'name (cadr call)
                                                        'arguments (let ([arguments (caddr call)])
                                                                     (if (string? arguments)
                                                                         arguments
                                                                         (jsexpr->string arguments)))))))))
  (hasheq 'choices (list (hasheq 'index 0 'message message 'finish_reason "stop"))))
