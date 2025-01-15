// SPDX-License-Identifier: MIT
// Source: https://github.com/gera2ld/tarjs
// constants.ts
export var TarFileType;
(function (TarFileType) {
    /** '\0' or '0' */
    TarFileType[TarFileType["File"] = 0] = "File";
    /** '5' */
    TarFileType[TarFileType["Dir"] = 53] = "Dir";
})(TarFileType || (TarFileType = {}));
// utils.ts
const encoder = new TextEncoder();
export const utf8Encode = (input) => encoder.encode(input);
export function getArrayBuffer(file) {
    if (typeof file === 'string')
        return utf8Encode(file).buffer;
    if (file instanceof ArrayBuffer)
        return file;
    if (ArrayBuffer.isView(file))
        return new Uint8Array(file).buffer;
    return file.arrayBuffer();
}
// writer.ts
export default class TarWriter {
    #fileData;
    constructor() {
        this.#fileData = [];
    }
    addFile(name, file, opts) {
        const data = getArrayBuffer(file);
        const size = data.byteLength ?? file.size;
        const item = {
            name,
            type: TarFileType.File,
            data,
            size,
            opts,
        };
        this.#fileData.push(item);
    }
    addFolder(name, opts) {
        this.#fileData.push({
            name,
            type: TarFileType.Dir,
            data: null,
            size: 0,
            opts,
        });
    }
    async write() {
        const buffer = createBuffer(this.#fileData);
        const view = new Uint8Array(buffer);
        let offset = 0;
        for (const item of this.#fileData) {
            // write header
            writeFileName(buffer, item.name, offset);
            writeFileType(buffer, item.type, offset);
            writeFileSize(buffer, item.size, offset);
            fillHeader(buffer, offset, item.opts, item.type);
            writeChecksum(buffer, offset);
            // write data
            const itemBuffer = await item.data;
            if (itemBuffer) {
                const data = new Uint8Array(itemBuffer);
                view.set(data, offset + 512);
            }
            offset += 512 + 512 * Math.floor((item.size + 511) / 512);
        }
        return new Blob([buffer], { type: 'application/x-tar' });
    }
}
export function createBuffer(fileData) {
    const dataSize = fileData.reduce((prev, item) => prev + 512 + 512 * Math.floor((item.size + 511) / 512), 0);
    const bufSize = 10240 * Math.floor((dataSize + 10240 - 1) / 10240);
    return new ArrayBuffer(bufSize);
}
function writeString(buffer, str, offset, size) {
    const bytes = utf8Encode(str);
    const view = new Uint8Array(buffer, offset, size);
    for (let i = 0; i < size; i += 1) {
        view[i] = i < bytes.length ? bytes[i] : 0;
    }
}
function writeFileName(buffer, name, offset) {
    // offset: 0
    writeString(buffer, name, offset, 100);
}
function writeFileType(buffer, type, offset) {
    // offset: 156
    const typeView = new Uint8Array(buffer, offset + 156, 1);
    typeView[0] = type;
}
function writeFileSize(buffer, size, offset) {
    // offset: 124
    const sizeStr = size.toString(8).padStart(11, '0');
    writeString(buffer, sizeStr, offset + 124, 12);
}
function writeFileMode(buffer, mode, offset) {
    // offset: 100
    writeString(buffer, mode.toString(8).padStart(7, '0'), offset + 100, 8);
}
function writeFileUid(buffer, uid, offset) {
    // offset: 108
    writeString(buffer, uid.toString(8).padStart(7, '0'), offset + 108, 8);
}
function writeFileGid(buffer, gid, offset) {
    // offset: 116
    writeString(buffer, gid.toString(8).padStart(7, '0'), offset + 116, 8);
}
function writeFileMtime(buffer, mtime, offset) {
    // offset: 136
    writeString(buffer, mtime.toString(8).padStart(11, '0'), offset + 136, 12);
}
function writeFileUser(buffer, user, offset) {
    // offset: 265
    writeString(buffer, user, offset + 265, 32);
}
function writeFileGroup(buffer, group, offset) {
    // offset: 297
    writeString(buffer, group, offset + 297, 32);
}
function writeChecksum(buffer, offset) {
    const header = new Uint8Array(buffer, offset, 512);
    // fill checksum fields with space
    for (let i = 0; i < 8; i += 1) {
        header[148 + i] = 32;
    }
    // add up header bytes as checksum
    let chksum = 0;
    for (let i = 0; i < 512; i += 1) {
        chksum += header[i];
    }
    writeString(buffer, chksum.toString(8).padEnd(8, ' '), offset + 148, 8);
}
function fillHeader(buffer, offset, opts, fileType) {
    const { uid, gid, mode, mtime, user, group } = {
        uid: 0,
        gid: 0,
        mode: fileType === TarFileType.File ? 0o600 : 0o755,
        mtime: ~~(Date.now() / 1000),
        user: 'root',
        group: 'root',
        ...opts,
    };
    writeFileMode(buffer, mode, offset);
    writeFileUid(buffer, uid, offset);
    writeFileGid(buffer, gid, offset);
    writeFileMtime(buffer, mtime, offset);
    writeString(buffer, 'ustar', offset + 257, 6); // magic string
    writeString(buffer, '00', offset + 263, 2); // magic version
    writeFileUser(buffer, user, offset);
    writeFileGroup(buffer, group, offset);
}
