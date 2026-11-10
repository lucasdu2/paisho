#lang racket/base

;; Reading environment variables, such as API keys, from a .env file.

(require racket/string)

(provide read-env-file
         load-env-file!)

;; The variables a .env file at `path` sets, as an association list in file order. Each line
;; is `NAME=value`, optionally preceded by `export`; blank lines and lines starting with `#`
;; are skipped. A value in matching single or double quotes is taken as written between
;; them; an unquoted value is trimmed and loses a trailing ` # comment`.
(define (read-env-file path)
  (for*/list ([line (in-list (file->lines* path))]
              [binding (in-value (parse-line line))]
              #:when binding)
    binding))

;; Sets each variable of the .env file at `path` that the environment doesn't already set,
;; so a variable given in the shell overrides the file.
(define (load-env-file! path)
  (for ([binding (in-list (read-env-file path))])
    (unless (getenv (car binding))
      (putenv (car binding) (cdr binding)))))

(define (file->lines* path)
  (call-with-input-file path
    (lambda (in) (for/list ([line (in-lines in 'any)]) line))))

(define (parse-line line)
  (define trimmed (string-trim line))
  (cond
    [(or (string=? trimmed "") (string-prefix? trimmed "#")) #f]
    [(regexp-match #px"^(?:export\\s+)?([A-Za-z_][A-Za-z0-9_]*)\\s*=\\s*(.*)$" trimmed)
     => (lambda (m) (cons (cadr m) (parse-value (caddr m))))]
    [else #f]))

(define (parse-value raw)
  (cond
    [(regexp-match #px"^\"(.*)\"$" raw) => cadr]
    [(regexp-match #px"^'(.*)'$" raw) => cadr]
    [else (string-trim (regexp-replace #px"\\s+#.*$" raw ""))]))
