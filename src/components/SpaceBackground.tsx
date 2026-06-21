import { useEffect, useRef } from "react";

/**
 * Celaris ambient backdrop. A fixed full-window layer behind the whole app: the
 * native-4K space artwork (planet Celaris), a slow parallax drift + rotating
 * ring glow, and a lively animated canvas — twinkling stars, drifting dust,
 * frequent multi-directional shooting stars and the odd flaring star.
 */
export function SpaceBackground({ bg, video, space = true }: { bg?: string; video?: string; space?: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    if (!space) return; // photo themes: no starfield/shooting stars
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    let w = 0;
    let h = 0;
    let dpr = Math.min(window.devicePixelRatio || 1, 2);

    type Star = { x: number; y: number; r: number; base: number; tw: number; ph: number; flare: number };
    type Dust = { x: number; y: number; r: number; vx: number; vy: number; a: number };
    type Shot = { x: number; y: number; vx: number; vy: number; len: number; life: number; max: number; col: string };

    let stars: Star[] = [];
    let dust: Dust[] = [];
    let shots: Shot[] = [];

    function resize() {
      if (!canvas) return;
      dpr = Math.min(window.devicePixelRatio || 1, 2);
      w = canvas.clientWidth;
      h = canvas.clientHeight;
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);
      ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);

      const starCount = Math.round((w * h) / 4200);
      stars = Array.from({ length: starCount }, () => ({
        x: Math.random() * w,
        y: Math.random() * h,
        r: Math.random() * 1.4 + 0.2,
        base: Math.random() * 0.55 + 0.25,
        tw: Math.random() * 0.8 + 0.25,
        ph: Math.random() * Math.PI * 2,
        flare: 0,
      }));

      const dustCount = Math.round((w * h) / 26000);
      dust = Array.from({ length: dustCount }, () => ({
        x: Math.random() * w,
        y: Math.random() * h,
        r: Math.random() * 1.6 + 0.6,
        vx: (Math.random() - 0.5) * 0.08,
        vy: (Math.random() - 0.5) * 0.08 - 0.04,
        a: Math.random() * 0.25 + 0.05,
      }));
    }

    const COLORS = ["rgba(225, 210, 255,", "rgba(255, 200, 245,", "rgba(200, 220, 255,"];

    function spawnShot() {
      // Varied origins/headings so streaks come from several directions.
      const mode = Math.random();
      let x: number, y: number, ang: number;
      if (mode < 0.55) {
        // top → down-right
        x = Math.random() * w * 0.9;
        y = -20;
        ang = Math.PI / 5 + Math.random() * 0.3;
      } else if (mode < 0.8) {
        // left → down-right
        x = -30;
        y = Math.random() * h * 0.6;
        ang = Math.PI / 7 + Math.random() * 0.25;
      } else {
        // right → down-left
        x = w + 30;
        y = Math.random() * h * 0.5;
        ang = Math.PI - (Math.PI / 6 + Math.random() * 0.3);
      }
      const speed = Math.random() * 4 + 5;
      shots.push({
        x,
        y,
        vx: Math.cos(ang) * speed,
        vy: Math.sin(ang) * speed,
        len: Math.random() * 110 + 70,
        life: 0,
        max: Math.random() * 70 + 55,
        col: COLORS[(Math.random() * COLORS.length) | 0],
      });
    }

    let last = performance.now();
    let acc = 0;
    function frame(now: number) {
      const dt = Math.min(now - last, 50);
      last = now;
      ctx!.clearRect(0, 0, w, h);
      const t = now / 1000;

      // Drifting dust (soft parallax depth).
      for (const d of dust) {
        d.x += d.vx;
        d.y += d.vy;
        if (d.x < -5) d.x = w + 5;
        if (d.x > w + 5) d.x = -5;
        if (d.y < -5) d.y = h + 5;
        ctx!.globalAlpha = d.a;
        ctx!.fillStyle = "#c9b8ff";
        ctx!.beginPath();
        ctx!.arc(d.x, d.y, d.r, 0, Math.PI * 2);
        ctx!.fill();
      }

      // Twinkling stars, with occasional brief flares.
      for (const s of stars) {
        if (s.flare > 0) s.flare -= dt / 1000;
        else if (Math.random() < 0.00008) s.flare = 0.6;
        const a = s.base + Math.sin(t * s.tw + s.ph) * 0.3 + Math.max(0, s.flare);
        ctx!.globalAlpha = Math.max(0, Math.min(1, a));
        ctx!.fillStyle = "#e3dbff";
        ctx!.beginPath();
        ctx!.arc(s.x, s.y, s.r + (s.flare > 0 ? s.flare * 1.5 : 0), 0, Math.PI * 2);
        ctx!.fill();
        // Cross flare for the brightest flaring stars.
        if (s.flare > 0.25) {
          ctx!.globalAlpha = s.flare;
          ctx!.strokeStyle = "rgba(230, 220, 255, 0.8)";
          ctx!.lineWidth = 1;
          const L = 6 + s.flare * 8;
          ctx!.beginPath();
          ctx!.moveTo(s.x - L, s.y); ctx!.lineTo(s.x + L, s.y);
          ctx!.moveTo(s.x, s.y - L); ctx!.lineTo(s.x, s.y + L);
          ctx!.stroke();
        }
      }
      ctx!.globalAlpha = 1;

      // Frequent shooting stars (~ every 0.9s, sometimes a pair).
      acc += dt;
      if (acc > 900) {
        acc = 0;
        if (Math.random() < 0.8) spawnShot();
        if (Math.random() < 0.25) spawnShot();
      }
      shots = shots.filter((sh) => sh.life < sh.max && sh.x > -150 && sh.x < w + 150);
      for (const sh of shots) {
        sh.x += sh.vx;
        sh.y += sh.vy;
        sh.life += 1;
        const tx = sh.x - sh.vx * (sh.len / 8);
        const ty = sh.y - sh.vy * (sh.len / 8);
        const fade = 1 - sh.life / sh.max;
        const grad = ctx!.createLinearGradient(sh.x, sh.y, tx, ty);
        grad.addColorStop(0, `${sh.col} ${0.95 * fade})`);
        grad.addColorStop(1, "rgba(157, 92, 255, 0)");
        ctx!.strokeStyle = grad;
        ctx!.lineWidth = 2;
        ctx!.lineCap = "round";
        ctx!.beginPath();
        ctx!.moveTo(sh.x, sh.y);
        ctx!.lineTo(tx, ty);
        ctx!.stroke();
        ctx!.globalAlpha = fade;
        ctx!.fillStyle = "#f6f0ff";
        ctx!.beginPath();
        ctx!.arc(sh.x, sh.y, 1.8, 0, Math.PI * 2);
        ctx!.fill();
        ctx!.globalAlpha = 1;
      }

      raf = requestAnimationFrame(frame);
    }

    resize();
    window.addEventListener("resize", resize);
    raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", resize);
    };
  }, [space]);

  return (
    <div className={`space-bg ${space ? "" : "photo"}`} aria-hidden>
      {video ? (
        <video className="space-bg__img" src={video} autoPlay loop muted playsInline />
      ) : (
        <img className="space-bg__img" src={bg ?? "/celaris-bg.png"} alt="" />
      )}
      {space && <div className="space-bg__planet" />}
      {space && <canvas ref={canvasRef} className="space-bg__stars" />}
      <div className="space-bg__vignette" />
    </div>
  );
}
