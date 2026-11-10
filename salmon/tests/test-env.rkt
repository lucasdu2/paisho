#lang racket/base

(require racket/file
         rackunit
         "../env.rkt")

(define (env-file contents)
  (define path (make-temporary-file "salmon-env-~a"))
  (display-to-file contents path #:exists 'truncate)
  path)

(test-case "a .env file's variables are read, skipping blanks and comments"
  (define path
    (env-file (string-append "# keys\n"
                             "\n"
                             "OPENAI_API_KEY=sk-plain\n"
                             "export OTHER_KEY = spaced  # a comment\n"
                             "QUOTED=\"a # kept\"\n"
                             "SINGLE='b c'\n"
                             "EMPTY=\n"
                             "not a binding\n")))
  (check-equal? (read-env-file path)
                '(("OPENAI_API_KEY" . "sk-plain")
                  ("OTHER_KEY" . "spaced")
                  ("QUOTED" . "a # kept")
                  ("SINGLE" . "b c")
                  ("EMPTY" . "")))
  (delete-file path))

(test-case "the shell's variables override the file's"
  (define path (env-file "SALMON_TEST_SET=from-file\nSALMON_TEST_UNSET=from-file\n"))
  (parameterize ([current-environment-variables
                  (environment-variables-copy (current-environment-variables))])
    (putenv "SALMON_TEST_SET" "from-shell")
    (environment-variables-set! (current-environment-variables) #"SALMON_TEST_UNSET" #f)
    (load-env-file! path)
    (check-equal? (getenv "SALMON_TEST_SET") "from-shell")
    (check-equal? (getenv "SALMON_TEST_UNSET") "from-file"))
  (delete-file path))
