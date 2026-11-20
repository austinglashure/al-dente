// The label Sigma draws for a node. Sigma also draws the hover label from
// it, so a label blanked by zoom or fading would leave the hovered node
// unnamed; the node under the cursor always keeps its name.

/** Why a label might be changed or hidden. */
export interface LabelState {
  /** Hidden by semantic zoom (functions when zoomed far out). */
  quiet: boolean;
  /** Outside the current highlight (drawn faded). */
  faded: boolean;
  /** Under the cursor. */
  hovered: boolean;
  /** An overlay marker shown before the name. */
  mark?: string | null;
  pinned?: boolean;
}

/** The label to draw.
 *  Example: `shownLabel('checkout', {quiet: true, faded: false, hovered: false})` is `''`;
 *  with `hovered: true` it is `'checkout'`. */
export function shownLabel(label: string, s: LabelState): string {
  if ((s.quiet || s.faded) && !s.hovered) return '';
  const marked = s.mark ? `${s.mark}${label}` : label;
  return s.pinned ? `📌 ${marked}` : marked;
}
