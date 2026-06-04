import { Cable, CheckCircle2, Mic, MonitorSpeaker } from "lucide-react";
import { isVbCableDevice, type AudioDevice } from "@/lib/api";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";

type DeviceSelectProps = {
  label: string;
  value: string | null;
  placeholder: string;
  devices: AudioDevice[];
  onChange: (value: string) => void;
};

export function DeviceSelect({ label, value, placeholder, devices, onChange }: DeviceSelectProps) {
  const likelyCable = devices.filter((device) => device.isVirtualCableLike);
  const regular = devices.filter((device) => !device.isVirtualCableLike);
  const selectedDevice = devices.find((device) => device.id === value);

  return (
    <div className="grid min-w-0 gap-2">
      <div className="flex min-w-0 items-center justify-between gap-3">
        <span className="shrink-0 text-xs font-semibold uppercase text-muted-foreground">{label}</span>
        {selectedDevice?.isVirtualCableLike ? <Badge tone="run">{isVbCableDevice(selectedDevice) ? "VB-CABLE" : "Cable"}</Badge> : null}
      </div>
      <Select value={value ?? undefined} onValueChange={onChange}>
        <SelectTrigger className="min-w-0 overflow-hidden [&>span]:min-w-0 [&>span]:overflow-hidden [&>span]:truncate">
          <SelectValue placeholder={placeholder} />
        </SelectTrigger>
        <SelectContent className="w-[420px] max-w-[calc(100vw-32px)]">
          {likelyCable.length ? (
            <SelectGroup>
              <SelectLabel>Virtual Cable</SelectLabel>
              {likelyCable.map((device) => (
                <SelectItem key={device.id} value={device.id}>
                  <span className="flex min-w-0 max-w-full items-center gap-2">
                    <Cable className="h-4 w-4 shrink-0 text-primary" />
                    <span className="min-w-0 truncate" title={device.name}>
                      {device.name}
                    </span>
                  </span>
                </SelectItem>
              ))}
            </SelectGroup>
          ) : null}
          {likelyCable.length && regular.length ? <SelectSeparator /> : null}
          <SelectGroup>
            <SelectLabel>{likelyCable.length ? "Other Devices" : "Devices"}</SelectLabel>
            {regular.map((device) => (
              <SelectItem key={device.id} value={device.id}>
                <span className="flex min-w-0 max-w-full items-center gap-2">
                  {device.flow === "capture" ? (
                    <Mic className="h-4 w-4 shrink-0 text-muted-foreground" />
                  ) : (
                    <MonitorSpeaker className="h-4 w-4 shrink-0 text-muted-foreground" />
                  )}
                  <span className="min-w-0 truncate" title={device.name}>
                    {device.name}
                  </span>
                  {device.isDefault ? <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-accent" /> : null}
                </span>
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </div>
  );
}
