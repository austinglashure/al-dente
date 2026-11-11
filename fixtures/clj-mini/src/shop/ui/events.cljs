(ns shop.ui.events
  "re-frame events."
  (:require [re-frame.core :as rf]))

(rf/reg-event-db :cart/add
  (fn [db [_ sku]]
    (update-in db [:cart sku] (fnil inc 0))))

(rf/reg-event-fx :order/submit
  (fn [{:keys [db]} _]
    {:db (assoc db :submitting true)
     :dispatch [:order/submitted]}))

(rf/reg-event-db :order/submitted
  (fn [db _]
    (assoc db :submitting false)))
