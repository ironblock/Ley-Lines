export type Platform = "brimstone" | "larkspur";

export interface Stage {
  name: string;
  platform: Platform;
  retries: number;
}
