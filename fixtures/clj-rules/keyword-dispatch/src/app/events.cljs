(ns app.events
  (:require [re-frame.core :as rf]))

(rf/reg-cofx :now
  (fn [cofx _] (assoc cofx :now 0)))

(rf/reg-event-fx :order/submit
  [(rf/inject-cofx :now)]
  (fn [{:keys [db]} _]
    {:db db
     :dispatch [:order/submitted]
     :fx [[:dispatch [:order/log]]]}))

(rf/reg-event-db :order/submitted
  (fn [db _] db))

(rf/reg-event-db :order/log
  (fn [db _] db))

(rf/reg-event-fx :order/batch
  (fn [_ _] {:dispatch-n [[:order/submitted] [:order/log]]}))

(rf/reg-event-db ::local
  (fn [db _] db))

(defn dispatch-computed [evt]
  (rf/dispatch evt))

(defn dispatch-computed-key [k]
  (rf/dispatch [k]))

(defn dispatch-unregistered []
  (rf/dispatch [:no/handler]))

(rf/reg-fx :app/notify
  (fn [_] nil))

(rf/reg-event-fx :order/notify
  (fn [_ _]
    {:app/notify 1
     :fx [[:app/notify 2] [:db nil]]}))

(rf/reg-event-fx :order/forward
  (fn [_ [_ evt]]
    {:dispatch evt}))
