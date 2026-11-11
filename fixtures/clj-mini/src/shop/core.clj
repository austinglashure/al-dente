(ns shop.core
  "Entry point: prices, reserves and charges an order."
  (:require [shop.inventory :as inventory]
            [shop.model :as model]
            [shop.payment :as payment]
            [shop.plugins :as plugins]
            [shop.pricing :as pricing]
            [shop.util :as util]))

(println "loading shop.core")

(defn checkout
  "Prices, reserves and charges one order. Returns a summary string."
  [order]
  (util/with-timing "checkout"
    (let [total (pricing/price order)]
      (inventory/reserve! (:sku order) (:qty order))
      (payment/charge (:payment order) total)
      (model/order-summary order total))))

(defn -main [& _args]
  (plugins/audit-hook)
  (let [card (payment/->Card "4242")]
    (payment/charge card 1))
  (println (checkout {:sku "A1" :qty 1 :kind :book :price 10 :payment "acct-1"})))
