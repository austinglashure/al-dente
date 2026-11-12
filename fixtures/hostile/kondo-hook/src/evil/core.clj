(ns evil.core)

(defmacro defthing [sym & body]
  `(def ~sym ~@body))

(defthing widget 1)

(defn target [] :target)

(defn tricky [x] x)

(defn caller []
  (tricky 1))
