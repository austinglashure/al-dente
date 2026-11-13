;; A script with no ns form: its code lives in the namespace user.
(require (quote [clojure.string :as str]))

(defn shout [s] (str/upper-case s))

(let [x "hi"]
  (println (shout x)))
