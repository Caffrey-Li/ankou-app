import mark from "../../brand/mark.svg";
import markDark from "../../brand/mark-dark.svg";

/** 万象标志：深色主题下用浅色描边的版本，深色背景上看得清。 */
export function Logo({ size = 22, className }: { size?: number; className?: string }) {
  return (
    <>
      <img className={className ? `${className} logo-light` : "logo-light"} src={mark} alt="" width={size} height={size} />
      <img className={className ? `${className} logo-dark` : "logo-dark"} src={markDark} alt="" width={size} height={size} />
    </>
  );
}
