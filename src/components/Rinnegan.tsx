import { useEffect, useRef, useState } from "react";

/**
 * The Celaris Rinnegan — an almond eye whose rippled violet iris follows the
 * cursor, so it actually stares at you. Concentric rings + luminous pupil + glow.
 */
export function Rinnegan({ size = 320 }: { size?: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const [off, setOff] = useState({ x: 0, y: 0 });

  useEffect(() => {
    function onMove(e: MouseEvent) {
      const el = ref.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height / 2;
      // Direction to cursor, clamped so the iris stays within the eye.
      let dx = (e.clientX - cx) / (r.width / 2);
      let dy = (e.clientY - cy) / (r.height / 2);
      const len = Math.hypot(dx, dy);
      if (len > 1) {
        dx /= len;
        dy /= len;
      }
      setOff({ x: dx * 26, y: dy * 16 }); // in viewBox units
    }
    window.addEventListener("mousemove", onMove);
    return () => window.removeEventListener("mousemove", onMove);
  }, []);

  const rings = [14, 24, 35, 47, 60, 72];

  return (
    <div className="rinnegan" ref={ref} style={{ width: size, height: (size * 180) / 280 }}>
      <svg viewBox="0 0 280 180" width={size} height={(size * 180) / 280}>
        <defs>
          <radialGradient id="rin-iris" cx="50%" cy="44%" r="62%">
            <stop offset="0%" stopColor="#d8c4ff" />
            <stop offset="34%" stopColor="#a874ff" />
            <stop offset="70%" stopColor="#7b46e0" />
            <stop offset="100%" stopColor="#2e1d57" />
          </radialGradient>
          <clipPath id="rin-eye">
            <path d="M30 90 Q140 6 250 90 Q140 174 30 90 Z" />
          </clipPath>
        </defs>

        {/* eye interior */}
        <path d="M30 90 Q140 6 250 90 Q140 174 30 90 Z" fill="#0d0a17" />

        {/* iris (clipped to the eye, follows the cursor) */}
        <g clipPath="url(#rin-eye)">
          <g transform={`translate(${off.x} ${off.y})`}>
            <circle cx="140" cy="90" r="82" fill="url(#rin-iris)" />
            {rings.map((r, i) => (
              <circle
                key={r}
                className="rin-ring"
                cx="140"
                cy="90"
                r={r}
                fill="none"
                stroke="#241640"
                strokeWidth="1.4"
                opacity="0.85"
                style={{ animationDelay: `${i * 0.18}s` }}
              />
            ))}
            <circle cx="140" cy="90" r="9" fill="#0b0716" />
            <ellipse cx="150" cy="80" rx="6" ry="3.4" fill="#ffffff" opacity="0.85" transform="rotate(-25 150 80)" />
          </g>
          {/* upper-lid shadow for depth */}
          <path d="M30 90 Q140 6 250 90" fill="none" stroke="#000000" strokeOpacity="0.35" strokeWidth="10" />
        </g>

        {/* eyelid outline + lashes */}
        <path d="M30 90 Q140 6 250 90 Q140 174 30 90 Z" fill="none" stroke="#1a1430" strokeWidth="3" />
        <path d="M30 90 Q140 6 250 90" fill="none" stroke="#cdb6ff" strokeOpacity="0.5" strokeWidth="1.4" />
      </svg>
    </div>
  );
}
