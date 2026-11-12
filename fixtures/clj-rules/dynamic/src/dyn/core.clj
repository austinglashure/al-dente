(ns dyn.core
  (:require [dyn.target :as t]))

(defn by-resolve [] ((resolve 'dyn.target/handler)))

(defn by-requiring-resolve [] ((requiring-resolve 'dyn.target/other)))

(defn by-ns-resolve [] ((ns-resolve 'dyn.target 'handler)))

(defn by-find-var [] (find-var 'dyn.target/other))

(defn computed [sym] ((requiring-resolve sym)))

(defn computed-ns [ns-sym] (ns-resolve ns-sym 'handler))

(defn evaluate [s] (eval (read-string s)))

(defn load-it [s] (load-string s))

(defn load-a-file [path] (load-file path))

(defn load-reader-it [r] (load-reader r))

(defn load-lib [] (load "dyn/extra"))

(defn spread [f args] (apply f args))

(defn bump [] (alter-var-root #'t/counter inc))

(defn make-var [] (intern 'dyn.target 'made 1))

(defn stubbed [] (with-redefs [t/handler (constantly :stub)] (t/handler)))
