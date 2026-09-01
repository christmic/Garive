export interface ThreadFindResult {
  readonly matches: readonly HTMLElement[];
  readonly capped: boolean;
}

interface TextSpan {
  readonly node: Text;
  readonly start: number;
  readonly end: number;
}

export function clearThreadFindMatches(root: HTMLElement): void {
  const parents = new Set<Node>();
  for (const mark of root.querySelectorAll<HTMLElement>("mark[data-search-match]")) {
    const parent = mark.parentNode;
    if (!parent) continue;
    parents.add(parent);
    while (mark.firstChild) parent.insertBefore(mark.firstChild, mark);
    mark.remove();
  }
  for (const parent of parents) parent.normalize();
}

export function findThreadTextMatches(root: HTMLElement, query: string,
  maxMatches = 500): ThreadFindResult {
  const needle = query.trim();
  if (!needle || maxMatches <= 0) return { matches: [], capped: false };
  const units = [...root.querySelectorAll<HTMLElement>("[data-thread-find-unit]")];
  const targets = units.length ? units : [root];
  const matches: HTMLElement[] = [];
  let capped = false;
  for (const target of targets) {
    const result = findWithin(target, needle, maxMatches - matches.length);
    matches.push(...result.matches);
    if (result.capped || matches.length === maxMatches) {
      capped = result.capped || targets.indexOf(target) < targets.length - 1;
      break;
    }
  }
  return { matches, capped };
}

function findWithin(root: HTMLElement, query: string, limit: number): ThreadFindResult {
  if (limit <= 0) return { matches: [], capped: true };
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      if (!(node instanceof Text)) return NodeFilter.FILTER_REJECT;
      const parent = node.parentElement;
      return !parent || parent.closest("script, style, textarea, [contenteditable='true'], [data-thread-find-skip]")
        ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
    },
  });
  const spans: TextSpan[] = [];
  let length = 0;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!(node instanceof Text)) continue;
    const textLength = node.textContent?.length ?? 0;
    spans.push({ node, start: length, end: length + textLength });
    length += textLength;
  }
  const haystack = spans.map((span) => span.node.textContent ?? "").join("").toLocaleLowerCase();
  const needle = query.toLocaleLowerCase();
  const offsets: Array<{ start: number; end: number }> = [];
  let cursor = 0;
  while (cursor < haystack.length && offsets.length < limit) {
    const start = haystack.indexOf(needle, cursor);
    if (start < 0) break;
    offsets.push({ start, end: start + needle.length });
    cursor = start + needle.length;
  }
  const capped = offsets.length === limit && haystack.indexOf(needle, cursor) >= 0;
  const matches: HTMLElement[] = [];
  for (let index = offsets.length - 1; index >= 0; index -= 1) {
    const offset = offsets[index]!;
    const start = spanAt(spans, offset.start);
    const end = spanAt(spans, offset.end - 1);
    if (!start || !end) continue;
    const range = document.createRange();
    range.setStart(start.node, offset.start - start.start);
    range.setEnd(end.node, offset.end - end.start);
    const mark = document.createElement("mark");
    mark.dataset.searchMatch = "";
    mark.append(range.extractContents());
    range.insertNode(mark);
    matches.push(mark);
  }
  return { matches: matches.reverse(), capped };
}

function spanAt(spans: readonly TextSpan[], offset: number): TextSpan | undefined {
  return spans.find((span) => offset >= span.start && offset < span.end);
}
