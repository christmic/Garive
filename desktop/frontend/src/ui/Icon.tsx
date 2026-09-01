import type { SVGProps } from "react";

export type IconName =
  | "activity" | "agent" | "archive" | "check" | "chevron" | "close"
  | "command" | "copy" | "desktop" | "download" | "file" | "folder" | "grid" | "memory" | "more" | "panel" | "paperclip"
  | "plus" | "search" | "send" | "settings" | "shield" | "sparkle"
  | "source" | "stop" | "warning" | "work";

const paths: Record<IconName, string> = {
  activity: "M4 12h3l2-6 4 12 2-6h5",
  agent: "M8 9V7a4 4 0 0 1 8 0v2m-9 0h10a2 2 0 0 1 2 2v7H5v-7a2 2 0 0 1 2-2Zm2 4h.01M15 13h.01",
  archive: "M4 7h16M5 7l1 13h12l1-13M9 11h6M4 4h16v3H4z",
  check: "m5 12 4 4L19 6",
  chevron: "m9 18 6-6-6-6",
  close: "M6 6l12 12M18 6 6 18",
  command: "M9 6v12M15 6v12M6 9h12M6 15h12",
  copy: "M8 8h11v11H8zM5 16V5h11",
  desktop: "M4 5h16v12H4zM9 21h6M12 17v4",
  download: "M12 4v11m-4-4 4 4 4-4M5 20h14",
  file: "M6 3h8l4 4v14H6zM14 3v5h5",
  folder: "M3 6h7l2 2h9v11H3z",
  grid: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
  memory: "M9 4h6v3h3v10h-3v3H9v-3H6V7h3zM9 9h6v6H9z",
  more: "M5 12h.01M12 12h.01M19 12h.01",
  panel: "M4 5h16v14H4zM15 5v14",
  paperclip: "m9 13 5.5-5.5a3 3 0 0 1 4 4L11 19a5 5 0 0 1-7-7l7-7",
  plus: "M12 5v14M5 12h14",
  search: "m20 20-4.5-4.5M18 11a7 7 0 1 1-14 0 7 7 0 0 1 14 0z",
  send: "m4 4 17 8-17 8 4-8zM8 12h13",
  settings: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6Zm8 3 2-2-2-3-3 .5L15 5l-1-3h-4L9 5 7 7.5 4 7l-2 3 2 2-2 2 2 3 3-.5L9 19l1 3h4l1-3 2-2.5 3 .5 2-3z",
  shield: "M12 3 20 6v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z",
  sparkle: "m12 3 1.4 4.1L17 9l-3.6 1.9L12 15l-1.4-4.1L7 9l3.6-1.9zM5 15l.8 2.2L8 18l-2.2.8L5 21l-.8-2.2L2 18l2.2-.8zM19 14l.7 1.8 1.8.7-1.8.7L19 19l-.7-1.8-1.8-.7 1.8-.7z",
  source: "m9 7-5 5 5 5m6-10 5 5-5 5M14 4l-4 16",
  stop: "M8 8h8v8H8z",
  warning: "M12 3 2.5 20h19zM12 9v4M12 17h.01",
  work: "M5 7h14v13H5zM9 7V4h6v3M5 12h14M10 12v2h4v-2",
};

export function Icon({ name, ...props }: SVGProps<SVGSVGElement> & { readonly name: IconName }) {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d={paths[name]} />
    </svg>
  );
}
