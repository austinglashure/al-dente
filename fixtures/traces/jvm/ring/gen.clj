;; Reproduces ring-core's documented failure
;; `wrap-cookies-throws-exception-when-not-using-intervals-correctly-java`
;; (ring-core/test/ring/middleware/test/cookies.clj at the pinned commit),
;; which expects an AssertionError, and prints the trace. Run with gen.sh.
(require '[ring.middleware.cookies :refer [wrap-cookies]])
(import '[java.time Duration ZonedDateTime ZoneId])

;; Copied from the test namespace.
(defn zoned-date-time [& [year month day]]
  (ZonedDateTime/of year
                    (or month 1)
                    (or day 1)
                    0 0 0 0
                    (ZoneId/of "UTC")))

(let [cookies {"a" {:value "b", :path "/",
                    :secure true, :http-only true,
                    :expires (Duration/between (zoned-date-time 2012)
                                               (zoned-date-time 2015))}}
      handler (constantly {:cookies cookies})]
  (try
    ((wrap-cookies handler) {})
    (println "no exception")
    (catch AssertionError e
      (.printStackTrace e))))
