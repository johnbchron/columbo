(() => {
  // only initialize once, even if this script appears multiple times in the DOM
  if (window.__columbo) return;
  window.__columbo = true;

  // optional: window.__columboConfig = { swap: (placeholder, nodes) => { ... } }
  const {swap = (p, n) => p.replaceWith(...n)} = window.__columboConfig || {};

  // watch for streamed <template data-columbo-r-id> elements and swap them in
  new MutationObserver(mutations => {
    for (const {addedNodes} of mutations)
      for (const r of addedNodes) {
        const id = r.dataset?.columboRId;
        // ignore if not a replacement template
        if (!id) continue;

        // execute swap if placeholder exists, then delete replacement
        const p = document.querySelector(`[data-columbo-p-id="${id}"]`);
        if (p) swap(p, [...r.content.childNodes]);
        r.remove();
      }
  }).observe(document.documentElement, {childList: true, subtree: true});
})();
