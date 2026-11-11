(ns shop.plugins
  "Dynamic code: requiring-resolve on a literal and on a variable, and eval.")

(defn audit-hook []
  ((requiring-resolve 'shop.audit/record!) :plugin-loaded))

(defn load-plugin [sym-name]
  (let [f (requiring-resolve (symbol sym-name))]
    (f)))

(defn run-script [code]
  (eval (read-string code)))
