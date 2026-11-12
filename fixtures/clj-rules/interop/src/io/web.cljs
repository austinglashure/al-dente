(ns io.web
  (:require [goog.string :as gstr]))

(defn log [] (js/console.log "x"))

(defn method [el] (.focus el))

(defn prop [el] (.-value el))

(defn fmt [] (gstr/format "%s" 1))
