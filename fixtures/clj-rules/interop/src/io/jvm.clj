(ns io.jvm
  (:import (java.util ArrayList UUID)
           (java.io File)))

(defn static-call [] (Math/abs -1))

(defn ctor [] (ArrayList.))

(defn ctor-new [] (new File "x"))

(defn hinted [^String s] (.trim s))

(defn hinted-inline [x] (.length ^String x))

(defn unhinted [x] (.close x))

(defn uuid [] (UUID/randomUUID))

(defn cmp [] (proxy [java.util.Comparator] [] (compare [a b] 0)))
