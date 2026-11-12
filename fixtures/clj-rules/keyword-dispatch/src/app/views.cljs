(ns app.views
  (:require [re-frame.core :as rf]
            [re-frame.alpha :as rfa]
            [app.events :as ev]
            [app.subs]))

(defn order-list []
  @(rf/subscribe [:orders/all]))

(defn submit-button []
  (fn [] (rf/dispatch [:order/submit])))

(defn local-by-alias []
  (rf/dispatch-sync [::ev/local]))

(defn local-by-name []
  (rf/dispatch [:app.events/local]))

(defn alpha-log []
  (rfa/dispatch [:order/log]))
