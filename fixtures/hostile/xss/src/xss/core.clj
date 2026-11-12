(ns xss.core
  "Hostile case 4. The docstrings below must render as text in the GUI.")

(defn evil
  "<script>window.__pwned = 'script'</script><img src=x onerror=\"window.__pwned = 'onerror'\">"
  [x]
  (str x))

(defn caller []
  (evil 1))
