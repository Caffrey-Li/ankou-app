import type { InputHTMLAttributes } from "react";
import "./check.css";

export function Check(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className="check" type="checkbox" />;
}
