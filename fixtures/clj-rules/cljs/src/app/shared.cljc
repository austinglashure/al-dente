(ns app.shared)

(defn now []
  #?(:clj (System/currentTimeMillis)
     :cljs (js/Date.now)))

(defn both []
  (now))
