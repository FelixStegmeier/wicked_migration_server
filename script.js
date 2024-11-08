function autoResize(textarea) {
    textarea.style.height = "auto";
    textarea.style.height = textarea.scrollHeight + 'px';
  }

  function overwriteuploadedFiles(){
    uploadedFiles = []; 
    for(let child of document.getElementById('container').children){

      if(child.querySelector('#input-name').value.includes("xml")){
        var blob = new Blob([child.querySelector('#input-body').value], { type: 'text/plain' });
        var newFile = new File([blob], child.querySelector('#input-name').value, { type: 'text/xml' });
      }else{
        var blob = new Blob([child.querySelector('#input-body').value], { type: 'text/plain' });
        var newFile = new File([blob], child.querySelector('#input-name').value, { type: 'text/plain' });
      }
      uploadedFiles.push(newFile);
    }
  }

  let uploadedFiles = [];

  document.addEventListener('dragover', function (e) {
    e.preventDefault();
  });

  document.addEventListener('drop', function (e) {
    e.preventDefault();
  });
  

  let dropArea = document.getElementById('drop-area')
    ;['dragenter', 'dragover', 'dragleave', 'drop'].forEach(eventName => {
      dropArea.addEventListener(eventName, preventDefaults, false)
    })
  
  function preventDefaults(e) {
    e.preventDefault()
    e.stopPropagation()
  }

  ;['dragenter', 'dragover'].forEach(eventName => {
    dropArea.addEventListener(eventName, highlight, false)
  })

  ;['dragleave', 'drop'].forEach(eventName => {
    dropArea.addEventListener(eventName, unhighlight, false)
  })

  function highlight(e) {
    dropArea.classList.add('highlight')
  }

  function unhighlight(e) {
    dropArea.classList.remove('highlight')
  }

  dropArea.addEventListener('drop', handleDrop, false);

  function handleDrop(e) {
    e.preventDefault();
    e.stopPropagation();

    let dt = e.dataTransfer;
    let files = dt.files;
    let file_arr = Array.from(files);
    file_arr.forEach(addFile);
  }

  function addFile(newFile) {
    if (uploadedFiles.length == 0) {
      uploadedFiles.push(newFile);
      createAndAdd(newFile);
    }
    else {
        if(!newFileAlreadyExists(newFile)){
        uploadedFiles.push(newFile);
        createAndAdd(newFile);
      }
    }
  }
  function createAndAdd(newFile) {
    var templateRef = document.getElementById("template");
    
    let clone = templateRef.content.cloneNode(true);
    let wrapper = document.createElement("div");
    wrapper.appendChild(clone);

    let node = document.getElementById('container').appendChild(wrapper);

    node.querySelector('#removeButton').addEventListener('click', function(event){
      let element = event.target.parentNode.parentNode;
      element.parentNode.removeChild(element);
      let grandparent = parent.parentNode;
    });
    node.querySelector("#input-name").value = newFile.name;
    let reader = new FileReader()
    reader.onload = function(e) {
      node.querySelector("#input-body").value = e.target.result;
        setTimeout(() => {
        node.querySelector("#input-body").style.height = node.querySelector("#input-body").scrollHeight + 'px';
      }, 0);
    }

    reader.readAsText(newFile);
    node.querySelector("#input-body").style.height = node.querySelector("#input-body").scrollHeight + 'px';
  }

  function newFileAlreadyExists(newFile){
    let name = newFile.name;
    for(let child of document.getElementById('container').children){
      if(child.querySelector('#input-name').value === name){
        return true;
      }
    }
    return false;
  }
  function uploadedFiles_containsFile(newFile) {
    for (const file of uploadedFiles) {
      if (file.name === newFile.name) {
        return true;
      }
    }
    return false;
  }

  document.getElementById('fileInput').addEventListener('change', function (event) {
    let file_arr = [];
    for (let i = 0; i < event.target.files.length; i++) {
      file_arr.push(event.target.files[i]);
    }
    file_arr.forEach(addFile);
    event.target.value = "";
  });

  function clear_uploadedFiles() {
    uploadedFiles = [];
    document.getElementById('container').innerHTML = "";
  }

  function downloadURL(url, name) {
    var link = document.createElement("a");
    link.download = name;
    link.href = url;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    delete link;
  }
  document.getElementById('resetButton').addEventListener('click', function (event) {
    clear_uploadedFiles();
    document.getElementById('fileContent').textContent = "";
  });
  document.getElementById('submitButton').addEventListener('click', function (event) {
    overwriteuploadedFiles();

    let formData = new FormData();
    if (uploadedFiles.length > 0) {
      uploadedFiles.forEach(element => {
        formData.append('files[]', element);
      });

      fetch('/multipart', { method: 'POST', body: formData, })
        .then(
          response => {
            if (response.ok) {
              downloadURL(response.url, "nm-migrated.tar")
            }
            else {
              response.text().then(body => document.getElementById('fileContent').textContent = body).catch(e => document.getElementById('fileContent').textContent = e);
            }
          }
        ).catch(error => {
          document.getElementById('fileContent').textContent = "Network error occurred. Please try again.";
        });

      uploadedFiles = [];
      document.getElementById('fileContent').textContent = "";
    }
    else {
      document.getElementById('fileContent').textContent = "Please add a file first";
    }
  });