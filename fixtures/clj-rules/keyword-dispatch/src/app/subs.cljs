(ns app.subs
  (:require [re-frame.core :as rf]))

(rf/reg-sub :orders/all
  (fn [db _] (:orders db)))

(rf/reg-sub :orders/count
  :<- [:orders/all]
  (fn [orders _] (count orders)))
