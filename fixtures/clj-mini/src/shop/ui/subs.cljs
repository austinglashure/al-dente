(ns shop.ui.subs
  "re-frame subscriptions."
  (:require [re-frame.core :as rf]))

(rf/reg-sub :cart/items
  (fn [db _]
    (:cart db)))
