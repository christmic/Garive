import { Icon } from "./Icon";

export function ScrollToTailButton({ visible, working, label, onClick }: {
  readonly visible: boolean;
  readonly working: boolean;
  readonly label: string;
  readonly onClick: () => void;
}) {
  return <button className="conversation-tail-button" data-visible={visible}
    aria-hidden={!visible} aria-label={label} tabIndex={visible ? 0 : -1}
    type="button" onClick={visible ? onClick : undefined}>
    {working ? <span className="conversation-tail-working" aria-hidden="true">
      <span /><span /><span />
    </span> : <Icon name="chevron" />}
  </button>;
}
