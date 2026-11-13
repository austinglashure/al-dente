(ns app.core
  "Fails in a nested call so the traces have several app frames.")

(defn parse-qty [s]
  (let [n (js/parseInt s)]
    (when (js/isNaN n)
      (throw (js/Error. (str "bad qty: " s))))
    n))

(defn line-total [item]
  (* (:price item) (parse-qty (:qty item))))

(defn order-total [items]
  (reduce + (map line-total items)))

(defn valid?->ok! [items]
  (order-total items))

(defn main []
  (set! (.-stackTraceLimit js/Error) 50)
  (try
    (valid?->ok! [{:price 2 :qty "3"} {:price 5 :qty "x"}])
    (catch :default e
      (println (.-stack e)))))

(set! *main-cli-fn* main)
