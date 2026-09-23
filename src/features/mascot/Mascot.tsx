type MascotProps = {
  percentage: number;
};

export default function Mascot({ percentage }: MascotProps) {
  const mood = percentage > 50 ? "serein" : percentage > 20 ? "attentif" : "alerte";

  return (
    <div className={`mascot mascot--${mood}`} aria-label={`Mascotte ${mood}`} role="img">
      <div className="mascot__antenna" />
      <div className="mascot__face">
        <span className="mascot__eye" />
        <span className="mascot__eye" />
      </div>
      <div className="mascot__body" />
    </div>
  );
}
