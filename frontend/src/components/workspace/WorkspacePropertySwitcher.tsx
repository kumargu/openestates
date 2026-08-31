import {
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";

type WorkspacePropertyOption = {
  id: string;
  label: string;
  meta: string;
};

export function WorkspacePropertySwitcher({
  selectedId,
  homes,
  onSelect,
  triggerLabel,
}: {
  selectedId?: string;
  homes: WorkspacePropertyOption[];
  onSelect: (propertyId: string) => void;
  triggerLabel?: string;
}) {
  const [open, setOpen] = useState(false);
  const switcherRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listboxId = useId();
  const selectedHome = homes.find((home) => home.id === selectedId) ?? homes[0];

  useEffect(() => {
    if (!open) return undefined;
    const handlePointerDown = (event: PointerEvent) => {
      if (!switcherRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      triggerRef.current?.focus();
    };

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  if (!selectedHome) return null;

  if (homes.length === 1) {
    return (
      <div className="workspace-home-switcher workspace-home-switcher--single">
        <strong>{selectedHome.label}</strong>
        <span>{selectedHome.meta}</span>
      </div>
    );
  }

  const focusSelectedOption = () => {
    switcherRef.current
      ?.querySelector<HTMLButtonElement>('[role="option"][aria-selected="true"]')
      ?.focus();
  };

  const handleListKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const options = [...(switcherRef.current
      ?.querySelectorAll<HTMLButtonElement>(".workspace-home-switcher__option") ?? [])];
    if (options.length === 0) return;
    event.preventDefault();
    const activeIndex = options.findIndex((option) => option === document.activeElement);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? options.length - 1
        : event.key === "ArrowUp"
          ? (activeIndex - 1 + options.length) % options.length
          : (activeIndex + 1) % options.length;
    options[nextIndex]?.focus();
  };

  return (
    <div ref={switcherRef} className="workspace-home-switcher">
      <button
        ref={triggerRef}
        type="button"
        className="workspace-home-switcher__trigger"
        aria-label={`Switch home, currently ${selectedHome.label}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (!["ArrowDown", "ArrowUp"].includes(event.key)) return;
          event.preventDefault();
          if (!open) setOpen(true);
          requestAnimationFrame(focusSelectedOption);
        }}
      >
        <span className="workspace-home-switcher__identity">
          <strong>{triggerLabel ?? selectedHome.label}</strong>
          {triggerLabel ? null : <span>{selectedHome.meta}</span>}
        </span>
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="m4 6 4 4 4-4" />
        </svg>
      </button>
      {open && (
        <div
          id={listboxId}
          className="workspace-home-switcher__menu"
          role="listbox"
          aria-label="Saved homes"
          onKeyDown={handleListKeyDown}
        >
          {homes.map((home) => {
            const selected = home.id === selectedHome.id;
            return (
              <button
                key={home.id}
                type="button"
                className="workspace-home-switcher__option"
                role="option"
                aria-selected={selected}
                tabIndex={selected ? 0 : -1}
                onClick={() => {
                  setOpen(false);
                  onSelect(home.id);
                }}
              >
                <span>
                  <strong>{home.label}</strong>
                  <small>{home.meta}</small>
                </span>
                <span className="workspace-home-switcher__check" aria-hidden="true">
                  {selected ? "✓" : ""}
                </span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
