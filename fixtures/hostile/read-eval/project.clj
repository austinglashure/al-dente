;; Hostile case 5. Every form here would run code if this file were evaluated
;; or read with *read-eval* on. Al Dente reads it as data only.
(defproject evil "0.1.0"
  :source-paths ["src" #=(clojure.java.shell/sh "touch" "aldente-pwned-read-eval")]
  :test-paths ~(do (spit "aldente-pwned-unquote" "x") ["test"])
  :injections [#=(eval (spit "aldente-pwned-injection" "x"))]
  :main evil.core)
