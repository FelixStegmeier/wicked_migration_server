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
        file_arr.forEach(addFile);
        event.target.value = "";
        setFileDividers(document.getElementById('file-container'));
    });

    document.getElementById('reset-files-button').addEventListener('click', function(event) {
        clearFiles();
        showUserInfo("");
    });

    document.getElementById('submit-button').addEventListener('click', function(event) {

        if(!fileNamesAreValid()){
            return;
        }

        if(!alertIfFileContainsPassword()){
            return;
        }

        let filesContent = getFilesContent();
        if (filesContent.length <= 0) {
            showUserInfo("Please add a file first");
            return;
        }

        let formData = new FormData();
        filesContent.forEach(element => {
            formData.append('files[]', element);
        });

        fetch('/multipart', {
                method: 'POST',
                body: formData,
            })
            .then(
                response => {
                    if (response.ok) {
                        downloadURL(response.url, "nm-migrated.tar")
                    } else {
                        response.text().then(body => showUserInfo(body)).catch(e => showUserInfo(e));
                    }
                }
            ).catch(error => {
                showUserInfo("Network error occurred. Please try again.");
            });

        showUserInfo("");
    });
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

function getFilesContent() {
    let files = [];

    for (let child of getFiles(document.getElementById('file-container'))) {
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

function autoResize(textarea) {
    textarea.style.height = "auto";
    textarea.style.height = textarea.scrollHeight + 'px';
}

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
    file_arr.forEach(addFile);
    setFileDividers(document.getElementById('file-container'));
}

function addFile(newFile) {
    if (!newFileAlreadyExists(newFile)) {
        createAndAdd(newFile);
    }
}

function createAndAdd(newFile) {
    const templateRef = document.getElementById("file-template");
    let node = templateRef.content.cloneNode(true);

    const fileTextArea = node.querySelector("#file-content-textarea");
    fileTextArea.style.height = fileTextArea.scrollHeight + 'px';

    node.querySelector('#remove-button').addEventListener('click', function(event) {
        let element = event.target.closest("#file");
        element.parentNode.removeChild(element);
        setFileDividers(document.getElementById('file-container'));
        showOrHideFilePlaceholder();
    });

    node.querySelector("#file-name").value = newFile.name;
    let reader = new FileReader()
    reader.onload = function(e) {
        fileTextArea.value = e.target.result;
        setTimeout(() => {
            fileTextArea.style.height = fileTextArea.scrollHeight + 'px';
        }, 0);
    }
    reader.readAsText(newFile);

    let fileContainer = document.getElementById('file-container');
    fileContainer.appendChild(node);

    showOrHideFilePlaceholder();
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

function newFileAlreadyExists(newFile) {
    let name = newFile.name;
    for (let child of getFiles(document.getElementById('file-container'))) {
        if (child.querySelector('#file-name').value === name) {
            return true;
        }
    }
    return false;
}

function clearFiles() {
    document.getElementById('file-container').innerHTML = "";
    showOrHideFilePlaceholder();
}

function downloadURL(url, name) {
    let link = document.createElement("a");
    link.download = name;
    link.href = url;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    delete link;
}

function fileNamesAreValid(){
    let invalidNames = []

    for (let child of getFiles(document.getElementById('file-container'))) {
        let filename = child.querySelector('#file-name').value;
        if(!checkFilenameValidity(filename)){
            invalidNames.push(filename);        
        }
    }
    if(invalidNames.length > 0){
        alert("Invalid file names:\n" + invalidNames.join('\n') + "\nvalid name example: ifcfg-interfacename or something.xml")
        return false;
    }
    else{
        return true;
    }
    function checkFilenameValidity(filename) {
        let regex1 = /ifcfg-.+/i;
        let regex2 = /.+\.xml/i;
    
        return regex1.test(filename) || regex2.test(filename);
    }
}

function alertIfFileContainsPassword() {
    passwords = []
    let regex = /<passphrase>.+?<\/passphrase>|<password>.+?<\/password>|<client-key-passwd>.+?<\/client-key-passwd>|<key>.+?<\/key>|<modem-pin>.+?<\/modem-pin>|WIRELESS_WPA_PASSWORD=.+?$|WIRELESS_WPA_PSK=.+?$|WIRELESS_KEY_[0-3]=.+?$|WIRELESS_CLIENT_KEY_PASSWORD=.+?$|PASSWORD=.+?$/gms;

    for (let child of getFiles(document.getElementById('file-container'))) {
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
