import { useEffect, useRef, type InputHTMLAttributes } from "react";
import "./check.css";

export function Check({
  indeterminate = false,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { indeterminate?: boolean }) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = indeterminate;
  }, [indeterminate]);
  return <input {...props} ref={ref} className="check" type="checkbox" />;
}
