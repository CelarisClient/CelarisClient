import { useEffect, useRef } from "react";

/**
 * Procedural Matrix "digital rain" with 3 depth layers (far/mid/near — different
 * size, speed and brightness for parallax). Runs forever, so there is no loop
 * seam at all. Used as the background for the Matrix theme.
 */
export function MatrixRain() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const GLYPHS = "ｱｲｳｴｵｶｷｸｹｺｻｼｽｾﾀﾁﾂﾃﾅﾆﾇﾈﾊﾋﾌﾍﾎ0123456789:.=*+-<>";
    const glyph = () => GLYPHS[(Math.random() * GLYPHS.length) | 0];

    type Layer = { size: number; speed: number; color: string; head: string; drops: number[]; cols: number };
    let layers: Layer[] = [];
    let w = 0, h = 0;
    let dpr = Math.min(window.devicePixelRatio || 1, 2);
    let raf = 0;

    function makeLayer(size: number, speed: number, color: string, head: string): Layer {
      const cols = Math.ceil(w / size) + 1;
      const drops = Array.from({ length: cols }, () => Math.random() * -h);
      return { size, speed, color, head, drops, cols };
    }

    function resize() {
      if (!canvas) return;
      dpr = Math.min(window.devicePixelRatio || 1, 2);
      w = canvas.clientWidth;
      h = canvas.clientHeight;
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);
      ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);
      layers = [
        makeLayer(11, 1.4, "rgba(20, 110, 50, 0.85)", "#3be36b"), // far
        makeLayer(16, 2.4, "rgba(40, 200, 90, 0.9)", "#7dffa6"),  // mid
        makeLayer(24, 3.8, "rgba(90, 255, 140, 0.95)", "#daffe8"), // near
      ];
    }

    let last = performance.now();
    function frame(now: number) {
      const dt = Math.min((now - last) / 16.67, 3); // ~frames elapsed
      last = now;

      // Fade the whole canvas slightly → trailing tails.
      ctx!.fillStyle = "rgba(2, 8, 4, 0.12)";
      ctx!.fillRect(0, 0, w, h);

      for (const L of layers) {
        ctx!.font = `${L.size}px ui-monospace, monospace`;
        for (let i = 0; i < L.cols; i++) {
          const x = i * L.size;
          const y = L.drops[i];
          // bright head
          ctx!.fillStyle = L.head;
          ctx!.fillText(glyph(), x, y);
          // a body glyph just above in the layer colour
          ctx!.fillStyle = L.color;
          ctx!.fillText(glyph(), x, y - L.size);

          L.drops[i] += L.speed * L.size * 0.18 * dt + L.speed * dt;
          if (y > h && Math.random() > 0.975) {
            L.drops[i] = Math.random() * -120;
          }
        }
      }
      raf = requestAnimationFrame(frame);
    }

    resize();
    // Seed with a black frame so the first paint isn't empty.
    ctx.fillStyle = "#020804";
    ctx.fillRect(0, 0, w, h);
    window.addEventListener("resize", resize);
    raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", resize);
    };
  }, []);

  return (
    <div className="space-bg" aria-hidden style={{ background: "#020804" }}>
      <canvas ref={canvasRef} className="space-bg__stars" style={{ mixBlendMode: "normal" }} />
      <div className="space-bg__vignette" style={{ background: "radial-gradient(140% 120% at 50% 40%, transparent 50%, rgba(0,0,0,0.5) 100%)" }} />
    </div>
  );
}
