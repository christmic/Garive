import { useId, useRef, type KeyboardEvent } from "react";

export function ChoicePicker({ label, value, options, onChange }: {
  label: string;
  value: string;
  options: readonly (readonly [string, string])[];
  onChange: (value: string) => void;
}) {
  const labelId = useId();
  const buttons = useRef<Array<HTMLButtonElement | null>>([]);
  const move = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === "Home" ? 0 : event.key === "End" ? options.length - 1
      : event.key === "ArrowRight" || event.key === "ArrowDown"
        ? (index + 1) % options.length : (index - 1 + options.length) % options.length;
    const option = options[next];
    if (!option) return;
    onChange(option[0]);
    queueMicrotask(() => buttons.current[next]?.focus());
  };
  return <div className="field choice-field"><span id={labelId}>{label}</span>
    <div className="choice-options" role="group" aria-labelledby={labelId}>
      {options.map(([id, copy], index) => <button type="button" className="choice-option"
        aria-pressed={id === value} key={id} ref={(node) => { buttons.current[index] = node; }}
        tabIndex={id === value ? 0 : -1} onClick={() => onChange(id)}
        onKeyDown={(event) => move(event, index)}><span>{copy}</span><i aria-hidden="true" /></button>)}
    </div>
  </div>;
}
