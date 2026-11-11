(ns shop.inventory
  "Mutable stock in an atom.")

(defonce stock (atom {"A1" 5}))

(defn reserve! [sku qty]
  (swap! stock update sku - qty))

(defn restock! [sku qty]
  (swap! stock update sku (fnil + 0) qty))

(defn available [sku]
  (get @stock sku 0))
