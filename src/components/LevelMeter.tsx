interface Props {
  level: number; // 0..1
  active: boolean;
}

// A simple symmetric bar meter. Quiet when idle, reacts to mic input live.
export default function LevelMeter({ level, active }: Props) {
  const bars = 28;
  const peak = Math.max(0, Math.min(1, level));
  return (
    <div className="flex h-10 items-center justify-center gap-[3px]">
      {Array.from({ length: bars }).map((_, i) => {
        // Center bars are tallest; shape the response like a waveform.
        const dist = Math.abs(i - (bars - 1) / 2) / ((bars - 1) / 2);
        const envelope = 1 - dist * 0.7;
        const threshold = i / bars;
        const lit = active && peak * envelope > threshold * 0.5;
        const height = lit ? 8 + peak * envelope * 28 : 4;
        // When recording but quiet, bars breathe so the meter never looks frozen.
        // Stagger each bar's phase so it ripples rather than pulsing in unison.
        const breathe = active && !lit;
        return (
          <span
            key={i}
            className={`${lit ? "bg-accent" : breathe ? "bg-accent/40" : "bg-ink-700"}${breathe ? " echo-breathe" : ""}`}
            style={{
              width: 3,
              height,
              borderRadius: 999,
              transition: "height 80ms linear",
              boxShadow: lit ? "0 0 8px rgba(61,123,247,0.7)" : undefined,
              animationDelay: breathe ? `${(i % 7) * 0.12}s` : undefined,
            }}
          />
        );
      })}
    </div>
  );
}
