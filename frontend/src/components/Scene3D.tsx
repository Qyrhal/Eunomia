"use client";

// One small three.js scene shared by the 3D entity graph and the vector
// cloud: instanced spheres (cheap for thousands of points), optional link
// lines and labelled axes, orbit/zoom, hover tooltip, click to select.
// Coordinates can be any scale -- they're fitted into a unit sphere.

import { useEffect, useRef, useState } from "react";
import { Maximize, Minus, Plus } from "lucide-react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

export type ScenePoint = { id: string; x: number; y: number; z: number; color: string; label: string; size?: number };
export type SceneLink = { source: string; target: string };

type Props = {
  points: ScenePoint[];
  links?: SceneLink[];
  axes?: [string, string, string];
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  ariaLabel: string;
  /** draw every point's label (only sensible for small graphs) */
  labels?: boolean;
  /** floating zoom out / level / zoom in / reset bar, bottom right */
  zoomControls?: boolean;
};

const CLICK_SLOP = 4; // px a pointer may move and still count as a click
const MAX_LABELS = 80;

// "var(--kind-person)" -> the computed color; three can't read CSS variables.
// Resolved per theme: a cache lives only for one theme pass.
function cssColor(c: string, cache?: Map<string, THREE.Color>): THREE.Color {
  const hit = cache?.get(c);
  if (hit) return hit;
  const m = c.match(/^var\((--[^)]+)\)$/);
  const value = m ? getComputedStyle(document.documentElement).getPropertyValue(m[1]).trim() : c;
  const color = new THREE.Color();
  try {
    color.setStyle(value || "gray");
  } catch {
    color.set(0x888888);
  }
  cache?.set(c, color);
  return color;
}

const START = new THREE.Vector3(1.8, 1.3, 1.8);

export default function Scene3D({ points, links = [], axes, selectedId, onSelect, ariaLabel, labels, zoomControls }: Props) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const overlayRef = useRef<HTMLDivElement>(null);
  const zoomTextRef = useRef<HTMLSpanElement>(null);
  const apiRef = useRef<{ zoom: (factor: number) => void; reset: () => void } | null>(null);
  const onSelectRef = useRef(onSelect);
  const selectedRef = useRef(selectedId);
  const [hover, setHover] = useState<{ label: string; x: number; y: number } | null>(null);

  useEffect(() => {
    onSelectRef.current = onSelect;
    selectedRef.current = selectedId;
  });

  useEffect(() => {
    const wrap = wrapRef.current;
    const overlay = overlayRef.current;
    if (!wrap || !overlay) return;

    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    } catch {
      const msg = document.createElement("p");
      msg.className = "absolute inset-0 flex items-center justify-center text-[12.5px]";
      msg.style.color = "var(--ink-faint)";
      msg.textContent = "This view needs WebGL, which this browser has turned off.";
      overlay.replaceChildren(msg);
      return;
    }
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    wrap.prepend(renderer.domElement);
    renderer.domElement.style.display = "block";

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(50, 1, 0.01, 100);
    camera.position.copy(START);
    const controls = new OrbitControls(camera, renderer.domElement);
    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    controls.enableDamping = true;
    controls.autoRotate = !still;
    controls.autoRotateSpeed = 0.6;
    controls.minDistance = 0.4;
    controls.maxDistance = 8;
    controls.addEventListener("start", () => (controls.autoRotate = false));

    // fit into a unit sphere around the centroid
    const n = points.length;
    const c = points.reduce((a, p) => [a[0] + p.x / n, a[1] + p.y / n, a[2] + p.z / n], [0, 0, 0]);
    const radius = Math.max(1e-9, ...points.map((p) => Math.hypot(p.x - c[0], p.y - c[1], p.z - c[2])));
    const pos = points.map((p) => new THREE.Vector3((p.x - c[0]) / radius, (p.y - c[1]) / radius, (p.z - c[2]) / radius));
    const index = new Map(points.map((p, i) => [p.id, i]));

    const baseSize = n > 400 ? 0.016 : links.length || labels ? 0.045 : 0.035;
    const mesh = new THREE.InstancedMesh(
      new THREE.SphereGeometry(1, 14, 10),
      new THREE.MeshStandardMaterial({ roughness: 0.55, metalness: 0.05 }),
      Math.max(n, 1)
    );
    mesh.count = n;
    const m4 = new THREE.Matrix4();
    const place = (i: number, scale: number) => {
      const s = baseSize * (points[i].size ?? 1) * scale;
      m4.makeScale(s, s, s).setPosition(pos[i]);
      mesh.setMatrixAt(i, m4);
    };
    points.forEach((_, i) => place(i, 1));
    scene.add(mesh);
    // line materials tinted from --ink-faint, re-tinted on theme change
    const faintLines: THREE.LineBasicMaterial[] = [];

    if (links.length) {
      const verts: number[] = [];
      for (const l of links) {
        const a = index.get(l.source);
        const b = index.get(l.target);
        if (a === undefined || b === undefined) continue;
        verts.push(pos[a].x, pos[a].y, pos[a].z, pos[b].x, pos[b].y, pos[b].z);
      }
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute(verts, 3));
      const mat = new THREE.LineBasicMaterial({ transparent: true, opacity: 0.4 });
      faintLines.push(mat);
      scene.add(new THREE.LineSegments(g, mat));
    }

    // axis lines + labels at their positive ends
    const axisEnds: THREE.Vector3[] = [];
    if (axes) {
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute([-1.15, 0, 0, 1.15, 0, 0, 0, -1.15, 0, 0, 1.15, 0, 0, 0, -1.15, 0, 0, 1.15], 3));
      // theme border colours are rgba; three ignores alpha, so opacity is set here
      const axisMat = new THREE.LineBasicMaterial({ transparent: true, opacity: 0.6 });
      faintLines.push(axisMat);
      scene.add(new THREE.LineSegments(g, axisMat));
      const grid = new THREE.GridHelper(2.3, 10);
      const gm = grid.material as THREE.LineBasicMaterial;
      gm.vertexColors = false;
      gm.transparent = true;
      gm.opacity = 0.12;
      faintLines.push(gm);
      grid.position.y = -1.15;
      scene.add(grid);
      axisEnds.push(new THREE.Vector3(1.22, 0, 0), new THREE.Vector3(0, 1.22, 0), new THREE.Vector3(0, 0, 1.22));
    }

    const applyTheme = () => {
      const cache = new Map<string, THREE.Color>();
      points.forEach((p, i) => mesh.setColorAt(i, cssColor(p.color, cache)));
      if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
      const faint = cssColor("var(--ink-faint)", cache);
      faintLines.forEach((m) => {
        m.color.copy(faint);
        m.needsUpdate = true;
      });
    };
    applyTheme();
    const themeWatch = new MutationObserver(applyTheme);
    themeWatch.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });

    scene.add(new THREE.AmbientLight(0xffffff, 1.4));
    const sun = new THREE.DirectionalLight(0xffffff, 1.6);
    sun.position.set(2, 3, 2);
    scene.add(sun);

    // HTML labels, repositioned every frame (no React re-render)
    overlay.replaceChildren();
    const labelled = labels && n <= MAX_LABELS ? points.map((_, i) => i) : [];
    const tag = (text: string, faint = false) => {
      const el = document.createElement("span");
      el.textContent = text;
      el.className = "scene3d-label";
      if (faint) el.style.color = "var(--ink-faint)";
      overlay.appendChild(el);
      return el;
    };
    const pointTags = labelled.map((i) => tag(points[i].label.length > 18 ? `${points[i].label.slice(0, 17)}…` : points[i].label));
    const axisTags = (axes ?? []).map((a) => tag(a, true));
    const selectedTag = tag("");
    // Figma-style selection frame around the selected node
    const selFrame = document.createElement("div");
    selFrame.className = "frame-selected";
    Object.assign(selFrame.style, { position: "absolute", left: "0", top: "0", display: "none" });
    overlay.appendChild(selFrame);

    const size = { w: 1, h: 1 };
    const resize = () => {
      size.w = wrap.clientWidth;
      size.h = wrap.clientHeight;
      renderer.setSize(size.w, size.h);
      camera.aspect = size.w / Math.max(size.h, 1);
      camera.updateProjectionMatrix();
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(wrap);

    const v = new THREE.Vector3();
    const project = (el: HTMLElement, p: THREE.Vector3, dy = 0) => {
      v.copy(p).project(camera);
      const hidden = v.z > 1;
      el.style.display = hidden ? "none" : "block";
      el.style.transform = `translate(-50%, 0) translate(${((v.x + 1) / 2) * size.w}px, ${((1 - v.y) / 2) * size.h + dy}px)`;
    };

    const focal = () => size.h / 2 / Math.tan((camera.fov * Math.PI) / 360);
    const baseDistance = START.length();
    let lastZoom = "";

    apiRef.current = {
      zoom: (factor) => {
        controls.autoRotate = false;
        const offset = camera.position.clone().sub(controls.target);
        const d = THREE.MathUtils.clamp(offset.length() * factor, controls.minDistance, controls.maxDistance);
        camera.position.copy(controls.target).add(offset.setLength(d));
      },
      reset: () => {
        controls.target.set(0, 0, 0);
        camera.position.copy(START);
        controls.autoRotate = !still;
      },
    };

    let lastSelected: number | undefined;
    let raf = 0;
    const frame = () => {
      raf = requestAnimationFrame(frame);
      controls.update();
      const sel = selectedRef.current ? index.get(selectedRef.current) : undefined;
      if (sel !== lastSelected) {
        if (lastSelected !== undefined) place(lastSelected, 1);
        if (sel !== undefined) place(sel, 1.8);
        mesh.instanceMatrix.needsUpdate = true;
        lastSelected = sel;
      }
      renderer.render(scene, camera);
      // labels sit just under each sphere's projected edge
      const f = focal();
      const below = (i: number, scale = 1) => (baseSize * (points[i].size ?? 1) * scale * f) / camera.position.distanceTo(pos[i]) + 3;
      labelled.forEach((i, k) => project(pointTags[k], pos[i], i === sel ? below(i, 1.8) + 6 : below(i)));
      axisEnds.forEach((p, k) => project(axisTags[k], p));
      if (sel !== undefined && !labelled.includes(sel)) {
        selectedTag.textContent = points[sel].label;
        project(selectedTag, pos[sel], below(sel, 1.8) + 6);
      } else selectedTag.style.display = "none";
      if (sel !== undefined) {
        v.copy(pos[sel]).project(camera);
        if (v.z > 1) selFrame.style.display = "none";
        else {
          const r = (baseSize * (points[sel].size ?? 1) * 1.8 * focal()) / camera.position.distanceTo(pos[sel]);
          const box = Math.round(2 * r + 10);
          selFrame.style.display = "block";
          selFrame.style.width = selFrame.style.height = `${box}px`;
          selFrame.style.transform = `translate(${((v.x + 1) / 2) * size.w - box / 2}px, ${((1 - v.y) / 2) * size.h - box / 2}px)`;
        }
      } else selFrame.style.display = "none";
      if (zoomTextRef.current) {
        const z = `${Math.round((baseDistance / camera.position.distanceTo(controls.target)) * 100)}%`;
        if (z !== lastZoom) zoomTextRef.current.textContent = lastZoom = z;
      }
    };
    frame();

    // hover + click via raycasting
    const ray = new THREE.Raycaster();
    const ndc = new THREE.Vector2();
    const pick = (e: PointerEvent) => {
      const r = renderer.domElement.getBoundingClientRect();
      ndc.set(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1);
      ray.setFromCamera(ndc, camera);
      return ray.intersectObject(mesh)[0]?.instanceId;
    };
    let down: { x: number; y: number } | null = null;
    const onDown = (e: PointerEvent) => (down = { x: e.clientX, y: e.clientY });
    const onUp = (e: PointerEvent) => {
      if (down && Math.hypot(e.clientX - down.x, e.clientY - down.y) < CLICK_SLOP) {
        const i = pick(e);
        if (i !== undefined) onSelectRef.current?.(points[i].id);
      }
      down = null;
    };
    const onMove = (e: PointerEvent) => {
      if (e.buttons) return;
      const i = pick(e);
      const r = wrap.getBoundingClientRect();
      setHover(i === undefined ? null : { label: points[i].label, x: e.clientX - r.left, y: e.clientY - r.top });
      renderer.domElement.style.cursor = i === undefined ? "grab" : "pointer";
    };
    const canvas = renderer.domElement;
    canvas.addEventListener("pointerdown", onDown);
    canvas.addEventListener("pointerup", onUp);
    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerleave", () => setHover(null));

    return () => {
      cancelAnimationFrame(raf);
      apiRef.current = null;
      themeWatch.disconnect();
      ro.disconnect();
      controls.dispose();
      renderer.dispose();
      scene.traverse((o) => {
        if (o instanceof THREE.Mesh || o instanceof THREE.LineSegments) {
          o.geometry.dispose();
          (o.material as THREE.Material).dispose();
        }
      });
      canvas.remove();
      overlay.replaceChildren();
    };
  }, [points, links, axes, labels]);

  return (
    <div className="relative w-full h-full overflow-hidden">
      <div ref={wrapRef} className="absolute inset-0" role="img" aria-label={ariaLabel}>
        <div ref={overlayRef} className="absolute inset-0 pointer-events-none" aria-hidden />
      </div>
      {hover && (
        <div
          className="panel absolute pointer-events-none px-2 py-1 text-[12px] max-w-xs"
          style={{ left: hover.x + 12, top: hover.y + 12, borderRadius: 6, color: "var(--ink)", boxShadow: "var(--shadow-pop)" }}
        >
          {hover.label}
        </div>
      )}
      {zoomControls && (
        <div className="panel absolute right-3 bottom-3 hidden md:flex items-center gap-0.5 p-1" role="group" aria-label="Zoom">
          <button type="button" className="btn btn-ghost btn-sm btn-icon w-[26px]" aria-label="Zoom out" onClick={() => apiRef.current?.zoom(1.25)}>
            <Minus size={14} strokeWidth={1.75} />
          </button>
          <span ref={zoomTextRef} className="font-mono text-[12px] w-12 text-center" style={{ color: "var(--ink-dim)" }}>
            100%
          </span>
          <button type="button" className="btn btn-ghost btn-sm btn-icon w-[26px]" aria-label="Zoom in" onClick={() => apiRef.current?.zoom(0.8)}>
            <Plus size={14} strokeWidth={1.75} />
          </button>
          <span className="w-px h-4 mx-0.5" style={{ background: "var(--border)" }} aria-hidden />
          <button type="button" className="btn btn-ghost btn-sm btn-icon w-[26px]" aria-label="Reset view" title="Reset view" onClick={() => apiRef.current?.reset()}>
            <Maximize size={14} strokeWidth={1.75} />
          </button>
        </div>
      )}
      {/* keyboard / screen-reader access to every point */}
      {onSelect && (
        <ul className="sr-only" aria-label={`${ariaLabel} items`}>
          {points.slice(0, 500).map((p) => (
            <li key={p.id}>
              <button type="button" onClick={() => onSelect(p.id)}>
                {p.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
