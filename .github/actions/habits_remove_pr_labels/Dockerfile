# Container image that runs your code
FROM python:3.7.3

RUN apt-get clean; apt-get update; apt-get install cmake pkg-config libicu-dev zlib1g-dev libcurl4-openssl-dev libssl-dev ruby-dev vim -y
RUN apt-get -y install git

#Dont need for pr remover
# RUN pip install nltk spacy
# # If I don't specify the download dir, they go to root and are unusable
# RUN python3 -c "import nltk; \
# 	nltk.download('punkt', download_dir='/usr/local/nltk_data/'); \
# 	nltk.download('averaged_perceptron_tagger', download_dir='/usr/local/nltk_data/'); \
# 	nltk.download('universal_tagset', download_dir='/usr/local/nltk_data/'); \
# 	nltk.download('stopwords', download_dir='/usr/local/nltk_data/'); \
# 	nltk.download('wordnet', download_dir='/usr/local/nltk_data/')" && \
# 	python3 -m spacy download en


#RUN gem install github-linguist

# Copies your code to container
COPY entrypoint.sh /entrypoint.sh

# Code file to execute when the container starts
ENTRYPOINT ["/entrypoint.sh"]