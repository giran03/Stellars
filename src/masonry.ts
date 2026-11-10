/**
 * Packs the children of a CSS grid into tight columns: each child spans as many 1px rows as it is
 * tall (plus `gap`), so the vertical gap between cards equals the horizontal one no matter how
 * tall each card is. Children keep their reading order; holes are filled by `grid-auto-flow: dense`.
 */
export function masonry(node: HTMLElement, gap = 16) {
  const resizeObserver = new ResizeObserver(layout);
  const observed = new Set<Element>();
  let frame = 0;

  function layout() {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      for (const child of Array.from(node.children) as HTMLElement[]) {
        child.style.gridRowEnd = `span ${Math.ceil(child.offsetHeight) + gap}`;
      }
    });
  }

  function watchChildren() {
    const current = new Set<Element>(Array.from(node.children));
    for (const child of current) {
      if (!observed.has(child)) {
        resizeObserver.observe(child);
        observed.add(child);
      }
    }
    for (const child of Array.from(observed)) {
      if (!current.has(child)) {
        resizeObserver.unobserve(child);
        observed.delete(child);
      }
    }
    layout();
  }

  const mutationObserver = new MutationObserver(watchChildren);
  mutationObserver.observe(node, { childList: true });
  resizeObserver.observe(node);
  watchChildren();

  return {
    destroy() {
      cancelAnimationFrame(frame);
      resizeObserver.disconnect();
      mutationObserver.disconnect();
    },
  };
}
