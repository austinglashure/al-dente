;; Writes traces produced by fixtures/clj-mini's own code. Run with gen.sh.
(require 'shop.core 'shop.inventory 'shop.interop 'shop.model 'shop.plugins
         '[clojure.java.io :as io])

(defn trace-of [f]
  (try (f) nil
       (catch Throwable e
         (let [sw (java.io.StringWriter.)]
           (.printStackTrace e (java.io.PrintWriter. sw))
           [e (str sw)]))))

(defn write! [name f]
  (let [[e text] (trace-of f)]
    (spit (io/file name) text)
    (println "wrote" name)
    e))

(def order {:sku "A1" :qty 1 :kind :book :price 10 :payment "acct-1"})

;; The :book method of the price multimethod calls half-off on nil.
(write! "checkout-book-nil-price.txt" #(shop.core/checkout (assoc order :price nil)))
;; reserve! subtracts a string from a number inside swap!.
(write! "checkout-bad-qty.txt" #(shop.core/checkout (assoc order :kind :gift :qty "x")))
;; No Chargeable implementation for a keyword.
(write! "checkout-no-charge-impl.txt" #(shop.core/checkout (assoc order :kind :gift :payment :cash)))
;; Interop on nil.
(write! "shout-nil.txt" #(shop.interop/shout nil))
;; A .cljc function.
(write! "format-money-string.txt" #(shop.model/format-money "x"))
;; eval reaches half-off; the static graph has no run-script -> half-off edge.
(let [e (write! "run-script-eval.txt" #(shop.plugins/run-script "(shop.pricing/half-off nil)"))]
  (spit "run-script-eval.edn" (with-out-str (prn (Throwable->map e))))
  (println "wrote run-script-eval.edn"))
