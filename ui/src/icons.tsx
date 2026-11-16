type IconSpec = {
  viewBox: string;
  size: number;
  strokeWidth: number;
  className?: string;
  paths: string[];
};

function Icon({ viewBox, size, strokeWidth, className, paths }: IconSpec) {
  return (
    <svg
      className={className ? `icon ${className}` : "icon"}
      width={size}
      height={size}
      viewBox={viewBox}
      fill="none"
      aria-hidden="true"
    >
      {paths.map((d) => (
        <path
          key={d}
          d={d}
          stroke="currentColor"
          strokeWidth={strokeWidth}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      ))}
    </svg>
  );
}

const nav = { viewBox: "0 0 17 17", size: 17, strokeWidth: 1.20417 };

export function IconHome() {
  return (
    <Icon
      {...nav}
      className="nav-icon"
      paths={[
        "M2.83333 7.4375L8.5 2.83333L14.1667 7.4375V14.1667C14.1667 14.3545 14.092 14.5347 13.9592 14.6675C13.8264 14.8004 13.6462 14.875 13.4583 14.875H10.2708V10.625H6.72917V14.875H3.54167C3.35381 14.875 3.17364 14.8004 3.0408 14.6675C2.90796 14.5347 2.83333 14.3545 2.83333 14.1667V7.4375Z",
      ]}
    />
  );
}

export function IconChart() {
  return (
    <Icon
      {...nav}
      className="nav-icon"
      paths={[
        "M3.54167 14.1667V9.20833M7.08333 14.1667V3.54167M10.625 14.1667V7.08333M14.1667 14.1667V10.625",
      ]}
    />
  );
}

export function IconBubble() {
  return (
    <Icon
      {...nav}
      className="nav-icon"
      paths={[
        "M3.54167 3.1875H13.4583C13.8341 3.1875 14.1944 3.33676 14.4601 3.60243C14.7257 3.86811 14.875 4.22844 14.875 4.60417V10.625C14.875 11.0007 14.7257 11.3611 14.4601 11.6267C14.1944 11.8924 13.8341 12.0417 13.4583 12.0417H8.5L4.95833 14.5208V12.0417H3.54167C3.16594 12.0417 2.80561 11.8924 2.53993 11.6267C2.27426 11.3611 2.125 11.0007 2.125 10.625V4.60417C2.125 4.22844 2.27426 3.86811 2.53993 3.60243C2.80561 3.33676 3.16594 3.1875 3.54167 3.1875Z",
      ]}
    />
  );
}

export function IconDisk() {
  return (
    <Icon
      {...nav}
      className="nav-icon"
      paths={[
        "M14.1667 4.60417C14.1667 5.69953 11.6296 6.5875 8.5 6.5875C5.37039 6.5875 2.83333 5.69953 2.83333 4.60417C2.83333 3.5088 5.37039 2.62083 8.5 2.62083C11.6296 2.62083 14.1667 3.5088 14.1667 4.60417Z",
        "M2.83333 4.60417V8.5M2.83333 8.5V12.3958C2.83333 13.4583 5.38333 14.3792 8.5 14.3792C11.6167 14.3792 14.1667 13.4583 14.1667 12.3958V8.5M2.83333 8.5C2.83333 9.5625 5.38333 10.4833 8.5 10.4833C11.6167 10.4833 14.1667 9.5625 14.1667 8.5M14.1667 8.5V4.60417",
      ]}
    />
  );
}

export function IconSet() {
  return (
    <Icon
      {...nav}
      className="nav-icon"
      paths={[
        "M2.83333 4.95833H9.20833M12.75 4.95833H14.1667M2.83333 12.0417H4.95833M8.5 12.0417H14.1667",
        "M12.6083 4.95833C12.6083 5.8581 11.8789 6.5875 10.9792 6.5875C10.0794 6.5875 9.35 5.8581 9.35 4.95833C9.35 4.05857 10.0794 3.32917 10.9792 3.32917C11.8789 3.32917 12.6083 4.05857 12.6083 4.95833Z",
        "M8.35833 12.0417C8.35833 12.9414 7.62893 13.6708 6.72917 13.6708C5.8294 13.6708 5.1 12.9414 5.1 12.0417C5.1 11.1419 5.8294 10.4125 6.72917 10.4125C7.62893 10.4125 8.35833 11.1419 8.35833 12.0417Z",
      ]}
    />
  );
}

export function IconClock() {
  return (
    <Icon
      viewBox="0 0 14 14"
      size={14}
      strokeWidth={0.991667}
      className="icon-clock"
      paths={[
        "M11.9583 7C11.9583 9.73841 9.73841 11.9583 7 11.9583C4.26159 11.9583 2.04167 9.73841 2.04167 7C2.04167 4.26159 4.26159 2.04167 7 2.04167C9.73841 2.04167 11.9583 4.26159 11.9583 7Z",
        "M7 4.375V7L8.75 8.16667",
      ]}
    />
  );
}

export function IconRefresh() {
  return (
    <Icon
      viewBox="0 0 15 15"
      size={15}
      strokeWidth={1.0625}
      className="icon-strong"
      paths={[
        "M12.1875 7.5C12.1885 8.58453 11.8133 9.63584 11.126 10.4748C10.4387 11.3137 9.48176 11.8884 8.41824 12.1008C7.35472 12.3133 6.25044 12.1504 5.29357 11.6399C4.3367 11.1294 3.58645 10.3029 3.17067 9.30121C2.75488 8.29955 2.69929 7.1847 3.01337 6.14664C3.32744 5.10859 3.99175 4.21156 4.89309 3.6084C5.79443 3.00525 6.87702 2.7333 7.9564 2.8389C9.03578 2.94449 10.0451 3.4211 10.8125 4.1875M12.1875 2.8125V5.625H9.375",
      ]}
    />
  );
}

export function IconChevron() {
  return (
    <Icon
      viewBox="0 0 14 14"
      size={14}
      strokeWidth={1.2}
      className="icon-strong"
      paths={["M5.25 3.5L8.75 7L5.25 10.5"]}
    />
  );
}

export function IconPlus() {
  return (
    <Icon viewBox="0 0 14 14" size={14} strokeWidth={1.4} className="icon-on-fill" paths={["M7 2.5V11.5", "M2.5 7H11.5"]} />
  );
}

export function IconCheck() {
  return (
    <Icon viewBox="0 0 13 13" size={13} strokeWidth={1.4} className="icon-strong" paths={["M2.5 6.8L5.2 9.4L10.5 3.6"]} />
  );
}

export function IconClose() {
  return (
    <Icon viewBox="0 0 14 14" size={14} strokeWidth={1.4} className="icon-strong" paths={["M3.5 3.5L10.5 10.5", "M10.5 3.5L3.5 10.5"]} />
  );
}

export function IconCopy() {
  return (
    <Icon
      viewBox="0 0 14 14"
      size={14}
      strokeWidth={1.2}
      className="icon-strong"
      paths={["M5 3.5H10.5V11H5V3.5Z", "M3.5 5V11.5H9"]}
    />
  );
}

export function IconArrow() {
  return (
    <Icon
      viewBox="0 0 14 14"
      size={14}
      strokeWidth={1.4}
      className="icon-on-fill"
      paths={["M2.5 7H11", "M8 4L11.2 7L8 10"]}
    />
  );
}

export function IconLock() {
  return (
    <Icon
      viewBox="0 0 15 15"
      size={15}
      strokeWidth={1.3}
      paths={["M4.5 7V4.8a3 3 0 0 1 6 0V7", "M3.5 7H11.5V12.5H3.5V7Z"]}
    />
  );
}

export function IconTrash() {
  return (
    <Icon
      viewBox="0 0 15 15"
      size={15}
      strokeWidth={1.3}
      paths={["M3.5 4.5H11.5", "M6 4.5V3.2H9V4.5", "M5 4.5L5.4 12H9.6L10 4.5"]}
    />
  );
}

export function IconSearch() {
  return (
    <Icon
      viewBox="0 0 15 15"
      size={15}
      strokeWidth={1.0625}
      className="icon-strong"
      paths={[
        "M10.625 6.875C10.625 8.94607 8.94607 10.625 6.875 10.625C4.80393 10.625 3.125 8.94607 3.125 6.875C3.125 4.80393 4.80393 3.125 6.875 3.125C8.94607 3.125 10.625 4.80393 10.625 6.875Z",
        "M12.5 12.5L9.6875 9.6875",
      ]}
    />
  );
}

export function IconCalendar() {
  return (
    <Icon
      viewBox="0 0 14 14"
      size={14}
      strokeWidth={0.991667}
      className="icon-calendar"
      paths={[
        "M3.5 2.625H10.5C11.1443 2.625 11.6667 3.14733 11.6667 3.79167V10.7917C11.6667 11.436 11.1443 11.9583 10.5 11.9583H3.5C2.85567 11.9583 2.33333 11.436 2.33333 10.7917V3.79167C2.33333 3.14733 2.85567 2.625 3.5 2.625Z",
        "M2.33333 5.54167H11.6667",
        "M4.95833 1.45833V3.20833",
        "M9.04167 1.45833V3.20833",
      ]}
    />
  );
}

export function IconAlert() {
  return (
    <Icon
      viewBox="0 0 16 16"
      size={16}
      strokeWidth={1.2}
      className="icon-alert"
      paths={[
        "M14 8C14 11.3137 11.3137 14 8 14C4.68629 14 2 11.3137 2 8C2 4.68629 4.68629 2 8 2C11.3137 2 14 4.68629 14 8Z",
        "M8 5V8.5",
        "M8 10.75V11",
      ]}
    />
  );
}

export function IconSpark() {
  return (
    <Icon
      viewBox="0 0 14 14"
      size={14}
      strokeWidth={1.1}
      className="icon-spark"
      paths={[
        "M7 1.75L8.1 5.2C8.2 5.5 8.5 5.8 8.8 5.9L12.25 7L8.8 8.1C8.5 8.2 8.2 8.5 8.1 8.8L7 12.25L5.9 8.8C5.8 8.5 5.5 8.2 5.2 8.1L1.75 7L5.2 5.9C5.5 5.8 5.8 5.5 5.9 5.2L7 1.75Z",
      ]}
    />
  );
}
