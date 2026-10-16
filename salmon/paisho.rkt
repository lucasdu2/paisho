#lang racket/base

;; A client for paisho's socket protocol: newline-delimited JSON, one response per request
;; (see `paisho/src/server.rs`).

(require json
         racket/unix-socket)

(provide (struct-out exn:fail:paisho)
         paisho-connect
         paisho-close
         paisho-request
         paisho-request/raw)

;; The server answered a request with an error.
(struct exn:fail:paisho exn:fail ())

;; One connection to the server. A connection runs one episode at a time, so use one per
;; parallel worker.
(struct connection (in out))

;; The address paisho listens on for the socket `name` (see `socket_name` in
;; `paisho/src/server.rs`): an abstract socket on Linux, a file in /tmp elsewhere.
(define (socket-address name)
  (case (system-type 'os*)
    [(linux) (bytes-append #"\0" (string->bytes/utf-8 name))]
    [(windows) (error 'paisho-connect "paisho's named pipes on Windows aren't supported")]
    [else (string-append "/tmp/" name)]))

(define (paisho-connect [name "paisho.sock"])
  (define-values (in out) (unix-socket-connect (socket-address name)))
  (connection in out))

(define (paisho-close conn)
  (close-input-port (connection-in conn))
  (close-output-port (connection-out conn)))

;; Sends `message` (a jsexpr) and returns the server's response, including errors.
(define (paisho-request/raw conn message)
  (define out (connection-out conn))
  (write-json message out)
  (newline out)
  (flush-output out)
  (define line (read-line (connection-in conn) 'linefeed))
  (when (eof-object? line)
    (error 'paisho-request "paisho closed the connection"))
  (string->jsexpr line))

;; Sends `message` and returns the server's response, raising `exn:fail:paisho` if it's an
;; error.
(define (paisho-request conn message)
  (define response (paisho-request/raw conn message))
  (if (equal? (hash-ref response 'type #f) "error")
      (raise (exn:fail:paisho (hash-ref response 'message) (current-continuation-marks)))
      response))
