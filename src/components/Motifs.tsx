import type { ReactElement } from "react";

/**
 * Celaris theme — background "motifs". One large, atmospheric SVG per menu,
 * crossfading as the active view changes. Motif language: the Titan Celaris
 * bearing the celestial sphere — orbital rings, constellations, star charts.
 */

const svgProps = {
  viewBox: "0 0 400 400",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

/** Play — the Titan Celaris bearing the celestial world (image asset, purple-tinted). */
function PlayMotif() {
  return <img className="motif-img" src="/celaris-titan.png" alt="" draggable={false} />;
}

/** Profiles — a constellation / star chart. */
function ProfilesMotif() {
  return (
    <svg {...svgProps}>
      <path d="M70 120 L150 90 L210 160 L300 130 L340 220" opacity="0.7" />
      <path d="M150 90 L180 200 L260 250 L300 130" opacity="0.5" />
      <path d="M180 200 L110 270 L210 160" opacity="0.5" />
      {[
        [70, 120], [150, 90], [210, 160], [300, 130], [340, 220],
        [180, 200], [260, 250], [110, 270],
      ].map(([x, y], i) => (
        <circle key={i} cx={x} cy={y} r={i % 3 === 0 ? 3 : 2} fill="currentColor" stroke="none" />
      ))}
    </svg>
  );
}

/** Mods — a modular orbital lattice of hex nodes. */
function ModsMotif() {
  const hex = (cx: number, cy: number, r: number) =>
    Array.from({ length: 6 }, (_, i) => {
      const a = (Math.PI / 3) * i - Math.PI / 6;
      return `${cx + r * Math.cos(a)},${cy + r * Math.sin(a)}`;
    }).join(" ");
  return (
    <svg {...svgProps}>
      <circle cx="200" cy="200" r="150" strokeDasharray="2 9" opacity="0.5" />
      {[
        [200, 70], [320, 150], [320, 290], [200, 330], [80, 290], [80, 150],
      ].map(([x, y], i) => (
        <polygon key={i} points={hex(x, y, 26)} opacity="0.8" />
      ))}
      <polygon points={hex(200, 200, 34)} />
    </svg>
  );
}

/** Settings — a great gear-ring orbit. */
function SettingsMotif() {
  const teeth = Array.from({ length: 16 }, (_, i) => {
    const a = (Math.PI / 8) * i;
    const x1 = 200 + 120 * Math.cos(a);
    const y1 = 200 + 120 * Math.sin(a);
    const x2 = 200 + 140 * Math.cos(a);
    const y2 = 200 + 140 * Math.sin(a);
    return <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} />;
  });
  return (
    <svg {...svgProps}>
      <circle cx="200" cy="200" r="120" />
      <circle cx="200" cy="200" r="60" />
      {teeth}
      <circle cx="200" cy="200" r="175" strokeDasharray="1 12" opacity="0.5" />
    </svg>
  );
}

const MOTIFS: Record<string, () => ReactElement> = {
  play: PlayMotif,
  profiles: ProfilesMotif,
  mods: ModsMotif,
  settings: SettingsMotif,
};

/** Stacks all motifs and crossfades to the one matching `view`. */
export function MotifBackground({ view }: { view: string }) {
  return (
    <div className="motif-layer" aria-hidden>
      {Object.entries(MOTIFS).map(([key, Motif]) => (
        <div key={key} className={`motif ${view === key ? "on" : ""}`}>
          <Motif />
        </div>
      ))}
    </div>
  );
}
