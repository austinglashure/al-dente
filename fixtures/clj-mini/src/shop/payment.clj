(ns shop.payment
  "A protocol with two records, an extend-type and a reify.")

(defprotocol Chargeable
  (charge [this amount] "Charges amount. Returns a receipt map."))

(defrecord Card [number]
  Chargeable
  (charge [_ amount]
    {:via :card :number number :amount amount}))

(defrecord Voucher [code]
  Chargeable
  (charge [_ amount]
    {:via :voucher :code code :amount (max 0 (- amount 5))}))

(extend-type String
  Chargeable
  (charge [s amount]
    {:via :account :account s :amount amount}))

(defn free-charger
  "A one-off implementation."
  []
  (reify Chargeable
    (charge [_ amount]
      {:via :free :amount 0 :waived amount})))
