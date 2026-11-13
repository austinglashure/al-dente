(ns app.core
  "Rich comment blocks: code that runs only when evaluated by hand.")

(defn helper [] 1)

(defn used [] (helper))

(comment
  (helper)
  (used)
  (eval '(helper)))

(defn also [] (helper))

(clojure.core/comment
  (also))
