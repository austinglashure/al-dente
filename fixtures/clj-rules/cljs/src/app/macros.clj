(ns app.macros)

(defmacro when-dev [& body]
  `(when true ~@body))
