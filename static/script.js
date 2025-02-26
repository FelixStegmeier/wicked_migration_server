import TarWriter from './tar_writer.js';

pageSetup();

function pageSetup() {
    const dropArea = document.getElementById('drop-area');

    document.addEventListener('dragover', function(e) {
        e.preventDefault();
    });

    document.addEventListener('drop', function(e) {
        e.preventDefault();
    });

    ['dragenter', 'dragover'].forEach(eventName => {
        dropArea.addEventListener(eventName, highlightDropArea, false)
    });

    ['dragleave', 'drop'].forEach(eventName => {
        dropArea.addEventListener(eventName, unhighlightDropArea, false)
    });

    ['dragenter', 'dragover', 'dragleave', 'drop'].forEach(eventName => {
        dropArea.addEventListener(eventName, preventDefaults, false)
    });

    dropArea.addEventListener('drop', handleDrop, false);

    document.getElementById('file-upload').addEventListener('change', function(event) {
        let file_arr = [];
        for (let i = 0; i < event.target.files.length; i++) {
            file_arr.push(event.target.files[i]);
        }
        file_arr.forEach(createAndAddWickedFile);
        event.target.value = "";
        setFileDividers(document.getElementById('file-container'));
    });

    document.getElementById('add-empty-file').addEventListener('click', function(event) {
        createAndAddWickedFile() 
    });

    document.getElementById('reset-files-button').addEventListener('click', function(event) {
        clearFiles('file-container');
        showOrHideFilePlaceholder();
        showUserInfo("");
    });

    document.getElementById('reset-configured-files-button').addEventListener('click', function(event) {
        clearFiles('file-result-container');
        showUserInfo("");
        showOrHideConfiguredFiles();
    });

    document.getElementById('download-nm-files-button').addEventListener('click', function(event) {
        downloadFiles();
    });

    document.getElementById('submit-button').addEventListener('click', function(event) {
        const fileElements = getFiles(document.getElementById('file-container'));
        const filesContent = getFilesContent(fileElements);

        if(!fileNamesAreValid(fileElements)){
            return;
        }

        if(!alertIfFileContainsPassword(fileElements)){
            return;
        }

        if(!alertIfFileIsEmpty(fileElements)){
            return
        }
        if (!alertIfDuplicateFileName(fileElements)){
            return;
        }
      
        if (filesContent.length <= 0) {        
            showUserInfo("Please add a file first");
            return;
        }

        let formData = new FormData();
        filesContent.forEach(element => {
            formData.append('files[]', element);
        });

        document.getElementById('submit-button').disabled = true;

        fetch('/json', {
            method: 'POST',
            body: formData,
        }).then(response => {
            if (response.ok) {
                response.json().then(json => {
                    clearFiles('file-result-container');

                    let parsed_json = JSON.parse(json);

                    showUserInfo(parsed_json.log)
                    parsed_json.files.forEach(file => {
                        createAndAddNMFile(file);
                        setFileDividers(document.getElementById('file-result-container'));
                    });
                    showOrHideConfiguredFiles();
                    document.getElementById('submit-button').disabled = false;
                })
                .catch(error => {
                    showUserInfo("Network error occurred. Please try again.");
                });;
            }
            else {
                response.text().then(data => {
                    showUserInfo("An error occured:\n"+ data);
                });
            }
        })

        showUserInfo("");
    });
}

function showOrHideConfiguredFiles(){
    if (configuratedContentIsEmpty()){
        hideConfigurationResult()
    }
    else{
        showConfigurationResult()
    }
}

function showConfigurationResult(){
    document.getElementById('download-nm-files-button').disabled = false;
    document.getElementById('migration-result-container').style.display= "block";
}

function hideConfigurationResult(){
    document.getElementById('download-nm-files-button').disabled = true;
    document.getElementById('migration-result-container').style.display = "none";
}

function configuratedContentIsEmpty() {
    if (getFiles(document.getElementById('file-result-container')).length > 0) {
        return false;
    }
    return true;
}

function showUserInfo(text) {
    const userInfo = document.getElementById('user-info');
    userInfo.textContent = text;
    if (text !== "") {
        userInfo.classList.add('highlight-error');
    } else {
        userInfo.classList.remove('highlight-error');
    }
}

function getFilesContent(fileElements) {
    let files = [];

    for (let child of fileElements) {
        let blob = new Blob([child.querySelector('#file-content-textarea').value], {
            type: 'text/plain'
        });
        let newFile

        if (child.querySelector('#file-name').value.includes("xml")) {
            newFile = new File([blob], child.querySelector('#file-name').value, {
                type: 'text/xml'
            });
        } else {
            newFile = new File([blob], child.querySelector('#file-name').value, {
                type: 'text/plain'
            });
        }
        files.push(newFile);
    }

    return files;
}

async function downloadFiles() {
    const tar_writer = new TarWriter();
    tar_writer.addFolder('system-connections');
    for (let child of getFiles(document.getElementById('file-result-container'))) {
        tar_writer.addFile('system-connections/' + child.querySelector('#file-name').value, child.querySelector('#file-content-textarea').value);
    }
    const output = await tar_writer.write();
    const fileURL = URL.createObjectURL(output);
    const downloadLink = document.createElement('a');
    downloadLink.href = fileURL;
    downloadLink.download = 'system-connections.tar';
    document.body.appendChild(downloadLink);
    downloadLink.click();
    URL.revokeObjectURL(fileURL);
}

function autoResize(textarea) {
    textarea.style.height = "auto";
    textarea.style.height = textarea.scrollHeight + 'px';
}
window.autoResize = autoResize;

function preventDefaults(e) {
    e.preventDefault()
    e.stopPropagation()
}

function highlightDropArea(e) {
    const dropArea = document.getElementById('drop-area');
    dropArea.classList.add('highlight');
}

function unhighlightDropArea(e) {
    const dropArea = document.getElementById('drop-area');
    dropArea.classList.remove('highlight');
}

function handleDrop(e) {
    e.preventDefault();
    e.stopPropagation();

    let dt = e.dataTransfer;
    let files = dt.files;
    let file_arr = Array.from(files);
    file_arr.forEach(createAndAddWickedFile);
    setFileDividers(document.getElementById('file-container'));
}

//Adds a wicked file to the dom if it doesn't exist already
async function createAndAddWickedFile(file = null) {
    let fileContent = ""
    let fileName = ""

    if (file) {
        fileContent = await file.text()
        fileName = file.name
    }

    createAndAddFile(fileName, fileContent, 'file-container', function(event) {
        let element = event.target.closest("#file");
        element.parentNode.removeChild(element);
        setFileDividers(document.getElementById('file-container'));
        showOrHideFilePlaceholder();
    });
    showOrHideFilePlaceholder();
}

// Adds a NM file to the dom
function createAndAddNMFile(file) {
    createAndAddFile(file.fileName, file.fileContent, 'file-result-container', function(event) {
        let element = event.target.closest("#file");
        element.parentNode.removeChild(element);
        setFileDividers(document.getElementById('file-result-container'));
        showOrHideConfiguredFiles();
    });
}

function createAndAddFile(filename, fileContent, containerId, deleteFunction) {
    const templateRef = document.getElementById("file-template");
    const node = templateRef.content.cloneNode(true);
    node.querySelector("#file-name").value = filename;
    const fileTextArea = node.querySelector("#file-content-textarea");
    fileTextArea.value = fileContent;
    node.querySelector('#remove-button').addEventListener('click', deleteFunction);
    const container = document.getElementById(containerId);
    container.appendChild(node);
    fileTextArea.style.height = fileTextArea.scrollHeight + 'px';
    setFileDividers(document.getElementById(containerId));
    return node
}

// Returns an array containing only all file elements of node
function getFiles(node) {
    return Array.from(node.children).filter((child) => child.className === "file");
}

// Removes all current dividers from node
// and then inserts divivders between all elements of node
function setFileDividers(node) {
    for (let divider of Array.from(node.children).filter((child) => child.className === "solid-divider")) {
        divider.remove();
    }
    let children = Array.from(node.children);
    children.pop();
    for (let child of children) {
        let newDivider = document.createElement('hr');
        newDivider.classList.add('solid-divider');
        node.insertBefore(newDivider, child.nextSibling);
    }
}

// If there are no files present a placeholder is shown, otherwise it gets hidden
function showOrHideFilePlaceholder() {
    document.getElementById('file-placeholder').hidden = getFiles(document.getElementById('file-container')).length != 0;
}

function clearFiles(target) {
    document.getElementById(target).innerHTML = "";
}

function downloadURL(url, name) {
    const link = document.createElement("a");
    link.download = name;
    link.href = url;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(link);
}

function fileNamesAreValid(files){
    let invalidNames = []
    let regex = /^ifcfg.+$|^ifroute-.+$|^routes$|^config$|^dhcp$|^.+\.xml$/i;

    for (let child of files) {
        let filename = child.querySelector('#file-name').value;
        if(!regex.test(filename)){
            invalidNames.push(filename);
        }
    }
    if(invalidNames.length > 0){
        alert("Invalid file names:\n" + invalidNames.join('\n') + "\nValid names are '<name>.xml' or wicked configuration files in '/etc/sysconfig/network' like e.g. 'ifcfg-<interfacename>', 'config', 'routes' or 'dhcp'")
        return false
    }
    else{
        return true;
    }
}

function alertIfFileContainsPassword(files) {
    let passwords = []
    let regex = /<passphrase>.+?<\/passphrase>|<password>.+?<\/password>|<client-key-passwd>.+?<\/client-key-passwd>|<key>.+?<\/key>|<modem-pin>.+?<\/modem-pin>|WIRELESS_WPA_PASSWORD=.+?$|WIRELESS_WPA_PSK=.+?$|WIRELESS_KEY_[0-3]=.+?$|WIRELESS_CLIENT_KEY_PASSWORD=.+?$|PASSWORD=.+?$/gms;

    for (let child of files) {
        let fileText = child.querySelector('#file-content-textarea').value;
        if (regex.test(fileText)){
            passwords.push(...fileText.match(regex));
        }
    }

    if(passwords.length > 0){
        let pswd_str = passwords.join('\n');
        return confirm("You have password(s) in your file. Consider removing it: " + pswd_str + "\n\nThis will be sent to the server, do you want to continue anyway?");
    }
    return true;
}

function alertIfFileIsEmpty(files) {
    for (let child of files) {
        let fileText = child.querySelector('#file-content-textarea').value;
        if (fileText.trim() == ""){
            return confirm("One of your files has no content, do you want to continue anyway?");
        }
    }
    return true
}

function alertIfDuplicateFileName(files) {
    let fileNames = []
    let duplicates = ""

    for (let child of files) {
        let fileName = child.querySelector('#file-name').value;
        if (fileNames.includes(fileName)){
            duplicates = duplicates.concat(fileName, "\n")
        }
        fileNames.push(fileName)
    }

    if(duplicates != ""){
        return confirm("You have duplicate config names:\n" + duplicates + "\nConfigs with duplicate names are ignored in the migration");
    }
    return true
}
