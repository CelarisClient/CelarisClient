import { useEffect, useRef } from "react";
import { SkinViewer, IdleAnimation } from "skinview3d";

/**
 * Real 3D Minecraft skin body (posed, gently breathing) for the launcher, via
 * skinview3d. Loads the player's skin by username from mc-heads (default skin
 * for unknown/offline names), or a supplied data URL (e.g. a just-applied PNG).
 */
export function SkinBody({
  username,
  width = 300,
  height = 440,
  skinDataUrl,
  slim,
  reloadKey = 0,
}: {
  username: string;
  width?: number;
  height?: number;
  /** If set, render this skin directly (e.g. a just-applied local PNG) instead of mc-heads. */
  skinDataUrl?: string;
  slim?: boolean;
  /** Bump to force a reload (cache-bust the remote skin after applying). */
  reloadKey?: number;
}) {
  const ref = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    if (!ref.current) return;
    let viewer: SkinViewer | null = null;
    try {
      viewer = new SkinViewer({ canvas: ref.current, width, height });
      const opts = slim ? { model: "slim" as const } : undefined;
      if (skinDataUrl) {
        viewer.loadSkin(skinDataUrl, opts).catch(() => {});
      } else {
        // Cache-bust so a freshly applied skin shows once Mojang has propagated it.
        const bust = reloadKey ? `?ts=${reloadKey}` : "";
        viewer
          .loadSkin(`https://mc-heads.net/skin/${encodeURIComponent(username || "MHF_Steve")}${bust}`, opts)
          .catch(() => {});
      }
      // Idle = subtle breathing + slight limb sway (no cursed T-pose / full spin).
      viewer.animation = new IdleAnimation();
      viewer.autoRotate = false;
      viewer.zoom = 0.9;
      // Pose: face slightly to a 3/4 view (keeps the default framing intact).
      viewer.playerObject.rotation.y = 0.5;
      if (viewer.controls) {
        viewer.controls.enableZoom = false;
        viewer.controls.enablePan = false;
      }
    } catch (e) {
      console.error("skin viewer", e);
    }
    return () => viewer?.dispose();
  }, [username, width, height, skinDataUrl, slim, reloadKey]);

  return (
    <div className="skin-body-wrap">
      <canvas ref={ref} className="skin-body" />
    </div>
  );
}
