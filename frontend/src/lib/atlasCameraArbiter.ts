export type AtlasCameraOwner = "society" | "road" | "nearby";

/** Allows only the active scene family to mutate the single persistent map camera. */
export class AtlasCameraArbiter {
  private activeOwner: AtlasCameraOwner = "society";

  activate(owner: AtlasCameraOwner): void {
    this.activeOwner = owner;
  }

  submit(owner: AtlasCameraOwner, apply: () => void): boolean {
    if (owner !== this.activeOwner) return false;
    apply();
    return true;
  }

  owner(): AtlasCameraOwner {
    return this.activeOwner;
  }
}
