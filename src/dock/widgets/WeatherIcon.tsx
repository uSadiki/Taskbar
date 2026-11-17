import type { Sky } from "../../lib/weatherCodes";

/**
 * Original weather icons drawn in a 32x32 box. Each has one quiet motion
 * (rays turning, clouds drifting, drops falling) that stops when Windows asks
 * for reduced motion. `animate` is off for small repeated icons, such as the
 * hourly forecast, where motion would be noise.
 */
export function WeatherIcon({
  sky,
  day = true,
  size = 24,
  animate = true,
}: {
  sky: Sky;
  day?: boolean;
  size?: number;
  animate?: boolean;
}) {
  const cls = (name: string) => (animate ? name : undefined);
  const sun = (
    <g className={cls("wx-spin")} style={{ transformOrigin: "11px 11px" }}>
      <circle cx="11" cy="11" r="5" fill="#ffc642" />
      {Array.from({ length: 8 }, (_, i) => (
        <rect
          key={i}
          x="10.2"
          y="1.5"
          width="1.6"
          height="3"
          rx="0.8"
          fill="#ffc642"
          transform={`rotate(${i * 45} 11 11)`}
        />
      ))}
    </g>
  );
  const moon = (
    <path d="M14.5 4.5a7 7 0 1 0 5 10.6 6 6 0 0 1-5-10.6z" fill="#e6e9f2" className={cls("wx-glow")} />
  );
  const celestial = day ? sun : moon;
  const cloud = (dark: boolean, cls2?: string) => (
    <path
      className={cls2}
      d="M9 27h14.5a5.5 5.5 0 0 0 .6-11A7.5 7.5 0 0 0 9.6 17.6 4.7 4.7 0 0 0 9 27z"
      fill={dark ? "#9aa3b5" : "#eef1f6"}
      stroke={dark ? "#7b8497" : "#cfd5e0"}
      strokeWidth="0.8"
    />
  );
  const drops = (count: number, color: string, length: number) => (
    <g>
      {Array.from({ length: count }, (_, i) => (
        <line
          key={i}
          className={cls("wx-fall")}
          style={{ animationDelay: `${i * 0.33}s` }}
          x1={11 + i * 5}
          y1={27.5}
          x2={10 + i * 5}
          y2={27.5 + length}
          stroke={color}
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      ))}
    </g>
  );
  const flakes = (
    <g fill="#ffffff">
      {[0, 1, 2].map((i) => (
        <circle
          key={i}
          className={cls("wx-fall")}
          style={{ animationDelay: `${i * 0.5}s` }}
          cx={11 + i * 5}
          cy={29}
          r="1.3"
        />
      ))}
    </g>
  );

  let body;
  switch (sky) {
    case "clear":
      body = day ? (
        <g transform="translate(5 5)">{sun}</g>
      ) : (
        <g transform="translate(3 3) scale(1.15)">{moon}</g>
      );
      break;
    case "partly":
      body = (
        <>
          <g transform="translate(-1 -1)">{celestial}</g>
          {cloud(false, cls("wx-drift"))}
        </>
      );
      break;
    case "cloudy":
      body = (
        <>
          <g transform="translate(-4 -5) scale(0.85)" opacity="0.8">
            {cloud(true)}
          </g>
          {cloud(false, cls("wx-drift"))}
        </>
      );
      break;
    case "fog":
      body = (
        <g stroke="#9aa3b5" strokeWidth="2.2" strokeLinecap="round">
          {[9, 15, 21].map((y, i) => (
            <line
              key={y}
              className={cls("wx-drift")}
              style={{ animationDelay: `${i * 0.6}s` }}
              x1={5 + (i % 2) * 3}
              y1={y}
              x2={26 - ((i + 1) % 2) * 3}
              y2={y}
            />
          ))}
        </g>
      );
      break;
    case "drizzle":
      body = (
        <g transform="translate(0 -4)">
          {cloud(false)}
          {drops(3, "#6fb6ff", 2)}
        </g>
      );
      break;
    case "rain":
    case "showers":
      body = (
        <g transform="translate(0 -4)">
          {sky === "showers" && <g transform="translate(-1 -1)">{celestial}</g>}
          {cloud(sky === "rain")}
          {drops(3, "#4f9dff", 4)}
        </g>
      );
      break;
    case "sleet":
      body = (
        <g transform="translate(0 -4)">
          {cloud(true)}
          {drops(2, "#4f9dff", 3)}
          <circle className={cls("wx-fall")} cx="22" cy="29" r="1.3" fill="#fff" />
        </g>
      );
      break;
    case "snow":
      body = (
        <g transform="translate(0 -4)">
          {cloud(false)}
          {flakes}
        </g>
      );
      break;
    case "thunder":
      body = (
        <g transform="translate(0 -3)">
          {cloud(true)}
          <path
            className={cls("wx-flash")}
            d="M16.5 22l-3.2 5.6h3l-1.6 5 5-6.6h-3.2l1.8-4z"
            fill="#ffd23f"
          />
        </g>
      );
      break;
  }

  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden className="wx-icon overflow-visible">
      {body}
    </svg>
  );
}
