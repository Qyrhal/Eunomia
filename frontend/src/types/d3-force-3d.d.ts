// d3-force-3d ships no types; it's d3-force's API with a z axis. Only what we use.
declare module "d3-force-3d" {
  export type Node3D = { id: string; x?: number; y?: number; z?: number };
  interface Force {
    strength(s: number): this;
    distance(d: number): this;
  }
  interface Simulation<N> {
    force(name: string, f: unknown): this;
    stop(): this;
    tick(n?: number): this;
    nodes(): N[];
  }
  export function forceSimulation<N extends Node3D>(nodes: N[], numDimensions?: number): Simulation<N>;
  export function forceLink<N extends Node3D>(links: { source: string; target: string }[]): Force & { id(fn: (n: N) => string): Force };
  export function forceManyBody(): Force;
  export function forceCenter(): Force;
}
