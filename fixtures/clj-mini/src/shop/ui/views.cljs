(ns shop.ui.views
  "Views: an npm string require, JS interop, dispatch and subscribe."
  (:require ["react" :as react]
            [re-frame.core :as rf]
            [shop.model :as model]
            [shop.ui.events]
            [shop.ui.subs]))

(defn cart-view []
  (let [items @(rf/subscribe [:cart/items])]
    (react/createElement "ul" nil (count items))))

(defn submit-button []
  (react/createElement "button"
                       #js {:onClick #(rf/dispatch [:order/submit])}
                       "Submit"))

(defn ^:export init []
  (js/console.log "init" (model/format-money 0))
  (rf/dispatch [:cart/add "A1"]))
