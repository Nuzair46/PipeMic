import type { AudioDevice } from "./types";

export function micSourceId(deviceId: string) {
  return `mic:${deviceId}`;
}

export function appSourceId(executable: string) {
  return `app:${executable.toLowerCase()}`;
}

export function isVbCableDevice(device: Pick<AudioDevice, "name" | "isVirtualCableLike"> | null | undefined) {
  if (!device?.isVirtualCableLike) {
    return false;
  }

  const name = device.name.toLowerCase();
  return name.includes("vb-audio") || name.includes("vb cable") || name.includes("vb-cable") || name.includes("cable input");
}

function isCanonicalVbCableInput(device: AudioDevice) {
  const name = device.name.toLowerCase();
  return isVbCableDevice(device) && name.includes("cable input") && !isSixteenChannelCableVariant(device);
}

function isSixteenChannelCableVariant(device: AudioDevice) {
  const name = device.name.toLowerCase();
  return name.includes("cable") && name.includes("16ch");
}

export function outputDevicesForPicker(devices: AudioDevice[]) {
  const canonicalVbCable = devices.find(isCanonicalVbCableInput);
  if (!canonicalVbCable) {
    return devices;
  }

  return devices.filter((device) => device.id === canonicalVbCable.id || !isSixteenChannelCableVariant(device));
}

export function preferredOutputDevice(devices: AudioDevice[]) {
  const selectableDevices = outputDevicesForPicker(devices);
  return (
    selectableDevices.find(isCanonicalVbCableInput) ??
    selectableDevices.find(isVbCableDevice) ??
    selectableDevices.find((device) => device.isVirtualCableLike) ??
    selectableDevices.find((device) => device.isDefault) ??
    selectableDevices[0] ??
    null
  );
}
