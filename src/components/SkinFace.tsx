/**
 * Renders a Minecraft skin's face (base layer + hat overlay) from a base64 PNG,
 * using CSS background cropping — works for both 64×64 and legacy 64×32 skins.
 */
export function SkinFace({ png, size = 72 }: { png: string; size?: number }) {
  const url = `url("data:image/png;base64,${png}")`;
  const layer = (xCells: number, yCells: number): React.CSSProperties => ({
    position: "absolute",
    inset: 0,
    width: size,
    height: size,
    backgroundImage: url,
    backgroundSize: `${8 * size}px`,
    backgroundPosition: `${-xCells * size}px ${-yCells * size}px`,
    imageRendering: "pixelated",
  });

  return (
    <div style={{ position: "relative", width: size, height: size }}>
      {/* face at (8,8); hat overlay at (40,8) — coords in 8px cells */}
      <div style={layer(1, 1)} />
      <div style={layer(5, 1)} />
    </div>
  );
}
