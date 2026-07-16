import { useEffect, useRef, useState, type PointerEvent } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

export interface ImagePreviewSource {
  src: string;
  alt: string;
}

export function ImagePreviewDialog({
  image,
  onOpenChange,
  nested = false,
}: {
  image: ImagePreviewSource | null;
  onOpenChange: (open: boolean) => void;
  /** Opened above another dialog — no mask, elevated shadow. */
  nested?: boolean;
}) {
  return (
    <Dialog
      open={Boolean(image)}
      onOpenChange={onOpenChange}
      modal={nested ? "trap-focus" : true}
    >
      <DialogPanel
        showOverlay={!nested}
        className={cn(
          "!h-[85vh] !max-h-[85vh] !w-[85vw] !max-w-[85vw]",
          nested && "todo-create-dialog"
        )}
      >
        <DialogHeader>
          <DialogTitle className="truncate">图片预览</DialogTitle>
        </DialogHeader>
        <DialogContent className="flex min-h-0 flex-1 flex-col overflow-hidden p-3 pt-0">
          {image && <ImagePreviewViewport src={image.src} alt={image.alt} />}
        </DialogContent>
      </DialogPanel>
    </Dialog>
  );
}

export function ImagePreviewViewport({ src, alt }: { src: string; alt: string }) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const zoomRef = useRef(1);
  const panRef = useRef({ x: 0, y: 0 });
  const dragRef = useRef<{
    startX: number;
    startY: number;
    originX: number;
    originY: number;
  } | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    zoomRef.current = 1;
    panRef.current = { x: 0, y: 0 };
    setZoom(1);
    setPan({ x: 0, y: 0 });
    setDragging(false);
    dragRef.current = null;
  }, [src]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const onWheel = (event: WheelEvent) => {
      event.preventDefault();

      const rect = viewport.getBoundingClientRect();
      const mouseX = event.clientX - rect.left;
      const mouseY = event.clientY - rect.top;
      const centerX = rect.width / 2;
      const centerY = rect.height / 2;

      const direction = event.deltaY < 0 ? 1 : -1;
      const currentZoom = zoomRef.current;
      const currentPan = panRef.current;
      const nextZoom = Math.min(
        5,
        Math.max(0.5, Number((currentZoom + direction * 0.15).toFixed(2))),
      );
      if (nextZoom === currentZoom) return;

      const ratio = nextZoom / currentZoom;
      const nextPan = {
        x: currentPan.x * ratio + (mouseX - centerX) * (1 - ratio),
        y: currentPan.y * ratio + (mouseY - centerY) * (1 - ratio),
      };

      zoomRef.current = nextZoom;
      panRef.current = nextPan;
      setZoom(nextZoom);
      setPan(nextPan);
    };

    viewport.addEventListener("wheel", onWheel, { passive: false });
    return () => viewport.removeEventListener("wheel", onWheel);
  }, [src]);

  const handlePointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    setDragging(true);
    dragRef.current = {
      startX: event.clientX,
      startY: event.clientY,
      originX: panRef.current.x,
      originY: panRef.current.y,
    };
  };

  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag) return;

    const nextPan = {
      x: drag.originX + event.clientX - drag.startX,
      y: drag.originY + event.clientY - drag.startY,
    };
    panRef.current = nextPan;
    setPan(nextPan);
  };

  const endDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (!dragRef.current) return;
    dragRef.current = null;
    setDragging(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  return (
    <div
      ref={viewportRef}
      className={cn(
        "flex min-h-0 flex-1 select-none overflow-hidden rounded-lg bg-foreground/[0.04] touch-none",
        dragging ? "cursor-grabbing" : "cursor-grab",
      )}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
    >
      <div className="flex h-full w-full items-center justify-center">
        <img
          src={src}
          alt={alt}
          draggable={false}
          className="max-h-full max-w-full origin-center object-contain will-change-transform"
          style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }}
        />
      </div>
    </div>
  );
}
