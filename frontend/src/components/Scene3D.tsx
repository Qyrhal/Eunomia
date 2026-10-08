"use client";

// The vector cloud's three.js scene: instanced spheres (cheap for thousands
// of points), labelled axes, orbit/zoom, hover tooltip, click to select.
// Coordinates can be any scale -- they're fitted into a unit sphere.

import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

export type ScenePoint = { id: string; x: number; y: number; z: number; color: string; label: string; size?: number };
type Props = {
  points: ScenePoint[];
  axes?: [string, string, string];
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  ariaLabel: string;
};

const CLICK_SLOP = 4; // px a pointer may move and still count as a click

// "var(--kind-person)" -> the computed color; three can't read CSS variables.
function cssColor(c: string): THREE.Color {
  const m = c.match(/^var\((--[^)]+)\)$/);
  const value = m ? getComputedStyle(document.documentElement).getPropertyValue(m[1]).trim() : c;
  return new THREE.Color(value || "#888888");
}

export default function Scene3D({ points, axes, selectedId, onSelect, ariaLabel }: Props) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const overlayRef = useRef<HTMLDivElement>(null);
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
    camera.position.set(1.8, 1.3, 1.8);
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.autoRotate = true;
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

    const baseSize = n > 400 ? 0.016 : 0.035;
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
    points.forEach((p, i) => {
      place(i, 1);
      mesh.setColorAt(i, cssColor(p.color));
    });
    scene.add(mesh);

    // axis lines + labels at their positive ends
    const axisEnds: THREE.Vector3[] = [];
    if (axes) {
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute([-1.15, 0, 0, 1.15, 0, 0, 0, -1.15, 0, 0, 1.15, 0, 0, 0, -1.15, 0, 0, 1.15], 3));
      // theme border colours are rgba; three ignores alpha, so opacity is set here
      scene.add(new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color: cssColor("var(--ink-faint)"), transparent: true, opacity: 0.6 })));
      const grid = new THREE.GridHelper(2.3, 10, cssColor("var(--ink-faint)"), cssColor("var(--ink-faint)"));
      const gm = grid.material as THREE.Material;
      gm.transparent = true;
      gm.opacity = 0.12;
      grid.position.y = -1.15;
      scene.add(grid);
      axisEnds.push(new THREE.Vector3(1.22, 0, 0), new THREE.Vector3(0, 1.22, 0), new THREE.Vector3(0, 0, 1.22));
    }

    scene.add(new THREE.AmbientLight(0xffffff, 1.4));
    const sun = new THREE.DirectionalLight(0xffffff, 1.6);
    sun.position.set(2, 3, 2);
    scene.add(sun);

    // HTML labels, repositioned every frame (no React re-render)
    overlay.replaceChildren();
    const tag = (text: string, faint = false) => {
      const el = document.createElement("span");
      el.textContent = text;
      el.className = "scene3d-label";
      if (faint) el.style.color = "var(--ink-faint)";
      overlay.appendChild(el);
      return el;
    };
    const axisTags = (axes ?? []).map((a) => tag(a, true));
    const selectedTag = tag("");

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
      axisEnds.forEach((p, k) => project(axisTags[k], p));
      if (sel !== undefined) {
        selectedTag.textContent = points[sel].label;
        project(selectedTag, pos[sel], 12);
      } else selectedTag.style.display = "none";
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
  }, [points, axes]);

  return (
    <div ref={wrapRef} className="relative w-full h-full overflow-hidden" role="img" aria-label={ariaLabel}>
      <div ref={overlayRef} className="absolute inset-0 pointer-events-none" aria-hidden />
      {hover && (
        <div
          className="absolute pointer-events-none px-2 py-1 rounded-md text-[11.5px] max-w-xs"
          style={{ left: hover.x + 12, top: hover.y + 12, background: "var(--surface-raised)", border: "1px solid var(--border)", color: "var(--ink)" }}
        >
          {hover.label}
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
