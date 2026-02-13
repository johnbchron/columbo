(function() {
  // Track templates waiting to be swapped in
  const pendingIds = new Set();
  // Prevent scheduling multiple microtasks for the same batch
  let transitionScheduled = false;

  // Swap placeholder with template content and clean up
  const performSwap = (id) => {
    const p = document.querySelector(`[data-columbo-p-id="${id}"]`);
    const r = document.querySelector(`[data-columbo-r-id="${id}"]`);
    
    if (p && r && p.parentNode) {
      p.parentNode.replaceChild(r.content, p);
      r.remove();
    }
  };

  // Process all pending swaps in a single View Transition (if available)
  const processQueue = () => {
    transitionScheduled = false;
    
    // Snapshot and clear the queue
    const ids = Array.from(pendingIds);
    pendingIds.clear();

    // Batch swaps in a View Transition for smooth updates
    if (document.startViewTransition) {
      document.startViewTransition(() => ids.forEach(performSwap));
    } else {
      ids.forEach(performSwap);
    }
  };

  // Watch for new template elements being added to the DOM
  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) {
        // Check if this is a columbo replacement template
        if (node.nodeType === 1 && node.hasAttribute('data-columbo-r-id')) {
          pendingIds.add(node.getAttribute('data-columbo-r-id'));
        }
      }
    }

    // Schedule processing if we have new templates and haven't already scheduled
    if (pendingIds.size > 0 && !transitionScheduled) {
      transitionScheduled = true;
      queueMicrotask(processQueue);
    }
  });

  // Observe the entire document to catch streamed chunks as they arrive
  observer.observe(document.documentElement, { childList: true, subtree: true });
})();
