export type TaggedMatrixItem = {
  group: string;
  primaryLabel: string;
};

export type TaggedMatrixColumn<T extends TaggedMatrixItem> = {
  key: string;
  items: readonly T[];
};

export type TaggedMatrixRow<T extends TaggedMatrixItem> = {
  tag: string;
  cells: Array<{
    columnKey: string;
    items: T[];
  }>;
};

export function buildTaggedMatrixRows<T extends TaggedMatrixItem>(
  columns: readonly TaggedMatrixColumn<T>[],
  group: string,
  compareTags: (left: string, right: string) => number,
  compareItems: (left: T, right: T) => number,
): TaggedMatrixRow<T>[] {
  const tags = new Set<string>();
  for (const column of columns) {
    for (const item of column.items) {
      if (item.group === group) tags.add(item.primaryLabel);
    }
  }

  return [...tags].sort(compareTags).map((tag) => ({
    tag,
    cells: columns.map((column) => ({
      columnKey: column.key,
      items: column.items
        .filter((item) => item.group === group && item.primaryLabel === tag)
        .sort(compareItems),
    })),
  }));
}
