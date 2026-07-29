export type NotebookCommand = {
  id: "visit" | "budget" | "payment" | "checklist";
  slash: string;
  title: string;
  keywords: string[];
  blockType: "checklist" | "fields";
  items?: string[];
  fields?: string[];
};

export const NOTEBOOK_COMMANDS: NotebookCommand[] = [
  {
    id: "visit",
    slash: "/visit",
    title: "Visit",
    keywords: ["site", "walkthrough", "inspection"],
    blockType: "checklist",
    items: [
      "Check water pressure",
      "Listen for balcony traffic noise",
      "Inspect basement for dampness",
      "Confirm parking slot",
      "Check kitchen utility space",
    ],
  },
  {
    id: "budget",
    slash: "/budget",
    title: "Budget",
    keywords: ["price", "emi", "loan", "cash"],
    blockType: "fields",
    fields: [
      "Asking price",
      "Down payment",
      "Loan needed",
      "Registration",
      "Immediate work",
      "Comfortable EMI",
    ],
  },
  {
    id: "payment",
    slash: "/payment",
    title: "Before payment",
    keywords: ["token", "legal", "readiness", "checklist"],
    blockType: "checklist",
    items: [
      "RERA registration reviewed",
      "Latest EC checked",
      "Parking allocation written",
      "Lawyer review complete",
      "Refund terms written",
    ],
  },
  {
    id: "checklist",
    slash: "/checklist",
    title: "Checklist",
    keywords: ["todo", "tasks", "items"],
    blockType: "checklist",
    items: ["New item"],
  },
];

export function slashQuery(value: string): string | null {
  const match = value.match(/^\/([a-z-]*)$/i);
  return match ? match[1].toLowerCase() : null;
}

export function matchingNotebookCommands(query: string): NotebookCommand[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return NOTEBOOK_COMMANDS;
  return NOTEBOOK_COMMANDS.filter((command) => {
    const haystack = [
      command.id,
      command.slash.slice(1),
      command.title,
      ...command.keywords,
    ].join(" ").toLowerCase();
    return haystack.includes(normalized);
  });
}
